// SPDX-License-Identifier: GPL-2.0-only
/*
 * epdc_uio.c -- the whole kernel side of the kindle-epdc userspace driver.
 *
 * Binds to the "mxc_epdc_fb" platform device (the one Amazon's board file registers for the i.MX50 EPDC)
 * once the stock mxc_epdc_fb module has been unloaded, and hands userspace three things through UIO:
 *   map0  the EPDC register window (4 KiB at 0x41010000), mapped uncached
 *   map1..map3  three physically contiguous, cache-coherent DMA buffers (default 2 MiB each; one 8 MiB ask only
 *         got 2 MiB on the device) for the waveform LUT data, the working buffer and the update pixel buffers
 *   irq   the EPDC interrupt (27): the handler just masks it at the interrupt controller and returns;
 *         userspace reads /dev/uioN to wait, services the EPDC status registers, and writes 1 to re-enable
 * It also turns on the two EPDC clocks and the EPDC pin mux, which on this 2.6.31 kernel can only be done
 * from kernel code. No EPDC register is touched here. Everything that knows what the EPDC is lives in userspace.
 *
 * Build: see ../README.md (external module against the ~/git/kindle-touch-kernel build tree).
 * Load:  /sbin/rmmod mxc_epdc_fb && /sbin/insmod /mnt/us/kindle-epdc/epdc_uio.ko
 */
#include <linux/module.h>
#include <linux/platform_device.h>
#include <linux/uio_driver.h>
#include <linux/interrupt.h>
#include <linux/dma-mapping.h>
#include <linux/clk.h>
#include <linux/io.h>

#define NBUF 3
static unsigned int dma_size = 2 << 20;
module_param(dma_size, uint, 0444);
MODULE_PARM_DESC(dma_size, "bytes per coherent DMA buffer; three are exposed as map1..map3 (default 2 MiB each; a buffer halves on failure)");

extern void gpio_epdc_pins_enable(int enable);

struct epdc_uio {
	struct uio_info info;
	struct clk *clk_axi;
	struct clk *clk_pix;
	void *dma_virt[NBUF];
	dma_addr_t dma_phys[NBUF];
	size_t dma_len[NBUF];
	int irq_enabled;
	int clocks_on;
};

/* /sys/bus/platform/devices/mxc_epdc_fb/clocks (also reachable as /sys/class/uio/uioN/device/clocks):
 * "1" while the shim holds epdc_axi+epdc_pix enabled. Userspace must refuse to touch map0 unless this reads 1:
 * a register access with the clocks gated stalls the AXI bus and wedges the CPU (7 Nov 2026 21:00). */
static ssize_t clocks_show(struct device *dev, struct device_attribute *attr, char *buf)
{
	struct epdc_uio *p = platform_get_drvdata(to_platform_device(dev));
	return sprintf(buf, "%d\n", p ? p->clocks_on : 0);
}
static DEVICE_ATTR(clocks, 0444, clocks_show, NULL);

/* Papyrus control pins, as the board file wires them (arch/arm/mach-mx5/mx50_pins.h, IOMUX_TO_GPIO = port*32+bit):
 * GPIO2_27 PWRCOM = VCOM enable (out), GPIO2_28 PWRSTAT = power good (in), GPIO2_29 PWRCTRL0 = wakeup (out).
 * The built-in papyrus regulator driver owns them, but gpio_{get,set}_value do not check ownership, so the shim
 * exposes them as sysfs files: vcom (rw), wakeup (rw), pwrgood (ro). Userspace follows papyrus-regulator.c's
 * sequence over I2C and these pins. */
#define GPIO_PWRCOM   (2 * 32 + 27)
#define GPIO_PWRSTAT  (2 * 32 + 28)
#define GPIO_PWRCTRL0 (2 * 32 + 29)
#include <linux/gpio.h>
static ssize_t gpio_attr_show(struct device *dev, struct device_attribute *attr, char *buf)
{
	int g = !strcmp(attr->attr.name, "vcom") ? GPIO_PWRCOM : !strcmp(attr->attr.name, "wakeup") ? GPIO_PWRCTRL0 : GPIO_PWRSTAT;
	return sprintf(buf, "%d\n", gpio_get_value(g) ? 1 : 0);
}
static ssize_t gpio_attr_store(struct device *dev, struct device_attribute *attr, const char *buf, size_t n)
{
	int g = !strcmp(attr->attr.name, "vcom") ? GPIO_PWRCOM : GPIO_PWRCTRL0;
	gpio_set_value(g, buf[0] == '1');
	return n;
}
static DEVICE_ATTR(vcom, 0644, gpio_attr_show, gpio_attr_store);
static DEVICE_ATTR(wakeup, 0644, gpio_attr_show, gpio_attr_store);
static DEVICE_ATTR(pwrgood, 0444, gpio_attr_show, NULL);

static irqreturn_t epdc_uio_handler(int irq, struct uio_info *info)
{
	struct epdc_uio *p = info->priv;
	/* Mask at the controller; userspace clears the EPDC's own status bits before re-enabling. */
	disable_irq_nosync(irq);
	p->irq_enabled = 0;
	return IRQ_HANDLED;
}

static int epdc_uio_irqcontrol(struct uio_info *info, s32 irq_on)
{
	struct epdc_uio *p = info->priv;
	if (irq_on && !p->irq_enabled) {
		p->irq_enabled = 1;
		enable_irq(info->irq);
	} else if (!irq_on && p->irq_enabled) {
		p->irq_enabled = 0;
		disable_irq(info->irq);
	}
	return 0;
}

static int __devinit epdc_uio_probe(struct platform_device *pdev)
{
	struct resource *regs, *irq;
	struct epdc_uio *p;
	int ret;

	regs = platform_get_resource(pdev, IORESOURCE_MEM, 0);
	irq = platform_get_resource(pdev, IORESOURCE_IRQ, 0);
	if (!regs || !irq) {
		dev_err(&pdev->dev, "epdc_uio: missing mem/irq resource\n");
		return -ENODEV;
	}

	p = kzalloc(sizeof(*p), GFP_KERNEL);
	if (!p)
		return -ENOMEM;

	p->clk_axi = clk_get(&pdev->dev, "epdc_axi");
	p->clk_pix = clk_get(&pdev->dev, "epdc_pix");
	if (IS_ERR(p->clk_axi) || IS_ERR(p->clk_pix)) {
		dev_err(&pdev->dev, "epdc_uio: clk_get failed (axi %ld, pix %ld)\n",
			PTR_ERR(p->clk_axi), PTR_ERR(p->clk_pix));
		ret = -ENODEV;
		goto err_free;
	}

	{
		int b;
		for (b = 0; b < NBUF; b++) {
			p->dma_len[b] = dma_size;
			while (p->dma_len[b] >= (1 << 20)) {
				p->dma_virt[b] = dma_alloc_coherent(&pdev->dev, p->dma_len[b], &p->dma_phys[b], GFP_KERNEL);
				if (p->dma_virt[b])
					break;
				p->dma_len[b] >>= 1;
			}
			if (!p->dma_virt[b]) {
				dev_err(&pdev->dev, "epdc_uio: dma_alloc_coherent for buffer %d failed down to 1 MiB\n", b);
				ret = -ENOMEM;
				goto err_dma_partial;
			}
			memset(p->dma_virt[b], 0, p->dma_len[b]);
		}
	}

	clk_enable(p->clk_axi);
	clk_enable(p->clk_pix);
	clk_set_rate(p->clk_pix, 32000000); /* E60_V220_WJ pixclock, as the stock init_hw sets it */
	p->clocks_on = 1;
	gpio_epdc_pins_enable(1);

	p->info.name = "epdc";
	p->info.version = "0.1";
	p->info.priv = p;
	p->info.mem[0].name = "regs";
	p->info.mem[0].addr = regs->start;
	p->info.mem[0].size = resource_size(regs);
	p->info.mem[0].memtype = UIO_MEM_PHYS;
	{
		int b;
		for (b = 0; b < NBUF; b++) {
			p->info.mem[1 + b].name = "dma";
			p->info.mem[1 + b].addr = p->dma_phys[b];
			p->info.mem[1 + b].size = p->dma_len[b];
			p->info.mem[1 + b].memtype = UIO_MEM_PHYS;
		}
	}
	p->info.irq = irq->start;
	p->info.irq_flags = 0;
	p->info.handler = epdc_uio_handler;
	p->info.irqcontrol = epdc_uio_irqcontrol;
	p->irq_enabled = 1;

	ret = uio_register_device(&pdev->dev, &p->info);
	if (ret) {
		dev_err(&pdev->dev, "epdc_uio: uio_register_device failed: %d\n", ret);
		goto err_dma;
	}

	platform_set_drvdata(pdev, p);
	if (device_create_file(&pdev->dev, &dev_attr_clocks) || device_create_file(&pdev->dev, &dev_attr_vcom) ||
	    device_create_file(&pdev->dev, &dev_attr_wakeup) || device_create_file(&pdev->dev, &dev_attr_pwrgood))
		dev_warn(&pdev->dev, "epdc_uio: could not create a sysfs attribute\n");
	dev_info(&pdev->dev, "epdc_uio: pix clk %lu Hz\n", clk_get_rate(p->clk_pix));
	dev_info(&pdev->dev, "epdc_uio: regs 0x%08lx+0x%lx irq %ld dma 0x%08lx+0x%zx 0x%08lx+0x%zx 0x%08lx+0x%zx -> see /sys/class/uio\n",
		 p->info.mem[0].addr, p->info.mem[0].size, p->info.irq,
		 (unsigned long)p->dma_phys[0], p->dma_len[0], (unsigned long)p->dma_phys[1], p->dma_len[1],
		 (unsigned long)p->dma_phys[2], p->dma_len[2]);
	return 0;

err_dma:
	p->clocks_on = 0;
	gpio_epdc_pins_enable(0);
	clk_disable(p->clk_pix);
	clk_disable(p->clk_axi);
err_dma_partial:
	{
		int b;
		for (b = 0; b < NBUF; b++)
			if (p->dma_virt[b])
				dma_free_coherent(&pdev->dev, p->dma_len[b], p->dma_virt[b], p->dma_phys[b]);
	}
	if (!IS_ERR(p->clk_axi)) clk_put(p->clk_axi);
	if (!IS_ERR(p->clk_pix)) clk_put(p->clk_pix);
err_free:
	kfree(p);
	return ret;
}

static int __devexit epdc_uio_remove(struct platform_device *pdev)
{
	struct epdc_uio *p = platform_get_drvdata(pdev);
	device_remove_file(&pdev->dev, &dev_attr_pwrgood);
	device_remove_file(&pdev->dev, &dev_attr_wakeup);
	device_remove_file(&pdev->dev, &dev_attr_vcom);
	device_remove_file(&pdev->dev, &dev_attr_clocks);
	uio_unregister_device(&p->info);
	p->clocks_on = 0;
	gpio_epdc_pins_enable(0);
	clk_disable(p->clk_pix);
	clk_disable(p->clk_axi);
	clk_put(p->clk_pix);
	clk_put(p->clk_axi);
	{
		int b;
		for (b = 0; b < NBUF; b++)
			dma_free_coherent(&pdev->dev, p->dma_len[b], p->dma_virt[b], p->dma_phys[b]);
	}
	kfree(p);
	return 0;
}

static struct platform_driver epdc_uio_driver = {
	.driver = { .name = "mxc_epdc_fb", .owner = THIS_MODULE },
	.probe = epdc_uio_probe,
	.remove = __devexit_p(epdc_uio_remove),
};

static int __init epdc_uio_init(void)
{
	return platform_driver_register(&epdc_uio_driver);
}
static void __exit epdc_uio_exit(void)
{
	platform_driver_unregister(&epdc_uio_driver);
}
module_init(epdc_uio_init);
module_exit(epdc_uio_exit);

MODULE_LICENSE("GPL");
MODULE_DESCRIPTION("UIO shim for the i.MX50 EPDC: registers, IRQ and a coherent buffer for a userspace driver");
