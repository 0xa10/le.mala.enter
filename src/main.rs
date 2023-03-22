#![no_std]
#![no_main]
#![feature(type_alias_impl_trait)]

use defmt::*;
use embassy_executor::Spawner;
use embassy_futures::join::join;
use embassy_futures::select::{select, Either};
use embassy_rp::gpio::{Input, Level, Output, Pull};
use embassy_rp::interrupt;
use embassy_rp::usb::Driver;
use embassy_usb::{Builder, Config};
use embassy_usb::class::hid::{HidReaderWriter, ReportId, RequestHandler, State};
use embassy_usb::control::OutResponse;
use usbd_hid::descriptor::{KeyboardReport, SerializedDescriptor};
use {defmt_rtt as _, panic_probe as _};

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let p = embassy_rp::init(Default::default());
     let mut led = Output::new(p.PIN_25, Level::Low);

    let irq = interrupt::take!(USBCTRL_IRQ);
    // Create the driver, from the HAL.
    let driver = Driver::new(p.USB, irq);

    // Create embassy-usb Config
    let mut config = Config::new(0x1209, 0x4853);
    config.manufacturer = Some("pini.grigio");
    config.product = Some("le.mala.enter");
    config.max_power = 100;
    config.max_packet_size_0 = 64;

    // Create embassy-usb DeviceBuilder using the driver and config.
    // It needs some buffers for building the descriptors.
    let mut device_descriptor = [0; 256];
    let mut config_descriptor = [0; 256];
    let mut bos_descriptor = [0; 256];
    let mut msos_descriptor = [0; 256];
    let mut control_buf = [0; 64];

    let mut state = State::new();

    let mut builder = Builder::new(
        driver,
        config,
        &mut device_descriptor,
        &mut config_descriptor,
        &mut bos_descriptor,
        &mut control_buf,
        &mut msos_descriptor,
    );

    // Create classes on the builder.
    let config = embassy_usb::class::hid::Config {
        report_descriptor: KeyboardReport::desc(),
        request_handler: None,
        poll_ms: 60,
        max_packet_size: 64,
    };
    let hid = HidReaderWriter::<_, 1, 8>::new(&mut builder, &mut state, config);

    // Build the builder.
    let mut usb = builder.build();

    // Run the USB device.
    let usb_fut = usb.run();

    let mut button = Input::new(p.PIN_20, Pull::Up);

    let (_, mut writer) = hid.split();

    // Do stuff with the class!
    let in_fut = async {
        loop {
            button.wait_for_low().await;
            info!("PRESSED");
            led.set_high();
            // Send up
            let report = KeyboardReport {
                keycodes: [0x52, 0, 0, 0, 0, 0],
                leds: 0,
                modifier: 0,
                reserved: 0,
            };
            match writer.write_serialize(&report).await {
                Ok(()) => {}
                Err(e) => warn!("Failed to send report: {:?}", e),
            };
            // Send enter
            let report = KeyboardReport {
                keycodes: [0x28, 0, 0, 0, 0, 0],
                leds: 0,
                modifier: 0,
                reserved: 0,
            };
            match writer.write_serialize(&report).await {
                Ok(()) => {}
                Err(e) => warn!("Failed to send report: {:?}", e),
            };

            button.wait_for_high().await;
            led.set_low();
            info!("RELEASED");
            let report = KeyboardReport {
                keycodes: [0, 0, 0, 0, 0, 0],
                leds: 0,
                modifier: 0,
                reserved: 0,
            };
            match writer.write_serialize(&report).await {
                Ok(()) => {}
                Err(e) => warn!("Failed to send report: {:?}", e),
            };
        }
    };

    // Run everything concurrently.
    // If we had made everything `'static` above instead, we could do this using separate tasks instead.
    join(usb_fut, in_fut).await;
}

struct MyRequestHandler {}

impl RequestHandler for MyRequestHandler {
    fn get_report(&self, id: ReportId, _buf: &mut [u8]) -> Option<usize> {
        info!("Get report for {:?}", id);
        None
    }

    fn set_report(&self, id: ReportId, data: &[u8]) -> OutResponse {
        info!("Set report for {:?}: {=[u8]}", id, data);
        OutResponse::Accepted
    }

    fn set_idle_ms(&self, id: Option<ReportId>, dur: u32) {
        info!("Set idle rate for {:?} to {:?}", id, dur);
    }

    fn get_idle_ms(&self, id: Option<ReportId>) -> Option<u32> {
        info!("Get idle rate for {:?}", id);
        None
    }
}

