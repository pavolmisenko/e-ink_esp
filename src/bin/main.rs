#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]
#![deny(clippy::large_stack_frames)]

use embedded_graphics::image::Image;
use embedded_graphics::pixelcolor::{BinaryColor, Rgb888};
use embedded_graphics::primitives::{Circle, Rectangle};
// use anyhow::Result;
use esp_backtrace as _;
use esp_hal::clock::CpuClock;
use esp_hal::gpio::{Input, InputConfig, Level, Output, OutputConfig, Pull};
use esp_hal::main;
use esp_hal::spi::Mode;
use esp_hal::time::{Rate};
use esp_hal::spi::master::{Config, Spi};
use esp_hal::timer::timg::TimerGroup;
use log::info;
use tinybmp::Bmp;

use embedded_hal_bus::spi::ExclusiveDevice;

use embedded_graphics::{prelude::*, primitives::{PrimitiveStyle}};
use epd_waveshare::{epd7in5b_v2::*, prelude::*};

// Add global allocator
use esp_alloc as _;

// This creates a default app-descriptor required by the esp-idf bootloader.
// For more information see: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_image_format.html#application-description>
esp_bootloader_esp_idf::esp_app_desc!();

#[allow(
    clippy::large_stack_frames,
    reason = "it's not unusual to allocate larger buffers etc. in main"
)]
#[main]
fn main() -> ! {

    // generator version: 1.1.0

    esp_println::logger::init_logger_from_env();

    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    // Disable BOTH watchdog timers to prevent reboots
    let mut timg0 = TimerGroup::new(peripherals.TIMG0);
    timg0.wdt.disable();
    let mut timg1 = TimerGroup::new(peripherals.TIMG1);
    timg1.wdt.disable();


    // Initialize the heap allocator
    esp_alloc::heap_allocator!(size: 64000);
    info!("Starting initialization...");

    // Create delay instance FIRST
    let mut delay = esp_hal::delay::Delay::new();
    info!("Delay created");

    // Initialize SPI bus
    let spi_bus = Spi::new(
        peripherals.SPI2,
        Config::default()
            .with_frequency(Rate::from_mhz(4))
            .with_mode(Mode::_0)
    ).unwrap()
    .with_sck(peripherals.GPIO18)
    .with_mosi(peripherals.GPIO23);
    info!("SPI bus created");

    // CS pin as output
    let cs = Output::new(peripherals.GPIO5, Level::High, OutputConfig::default());
    info!("CS pin created");

    // Wrap SPI bus with CS pin - use embedded_hal_bus::spi::NoDelay instead
    let mut spi_device = ExclusiveDevice::new_no_delay(spi_bus, cs).unwrap();
    info!("SPI device created");

    // Configure BUSY pin with pull-down to ensure stable LOW when idle
    let busy = Input::new(
        peripherals.GPIO4,
        InputConfig::default()
    );
    let dc = Output::new(peripherals.GPIO22, Level::High, OutputConfig::default());
    let rst = Output::new(peripherals.GPIO21, Level::High, OutputConfig::default());
    info!("GPIO pins initialized");

    // Check BUSY pin state before initialization
    info!("BUSY pin state (LOW=ready, HIGH=busy): {}", busy.is_high());

    // Manual reset sequence before EPD initialization
    info!("Performing manual reset...");
    let mut rst_mut = rst;
    rst_mut.set_low();
    delay.delay_millis(500);  // Increased from 200ms
    rst_mut.set_high();
    delay.delay_millis(500);  // Increased from 200ms
    info!("Reset complete, BUSY pin state: {}", busy.is_high());

    // Setup EPD
    info!("Attempting EPD initialization...");
    let mut epd = match Epd7in5::new(&mut spi_device, busy, dc, rst_mut, &mut delay, None) {
        Ok(epd) => {
            info!("EPD initialized successfully");
            epd
        },
        Err(e) => {
            info!("EPD initialization failed with error: {:?}", e);
            panic!("Failed to initialize EPD");
        }
    };

    //Set background to white
    epd.set_background_color(TriColor::White);
    let width = epd.width();
    let height = epd.height();
    info!("WIDTH = {}, HEIGHT = {}", width,  height);

    // Clear the display directly (this clears to white)
    // info!("Clearing display...");
    // epd.clear_frame(&mut spi_device, &mut delay).unwrap();
    // //epd.display_frame(&mut spi_device, &mut delay).unwrap();
    // epd.wait_until_idle(&mut spi_device, &mut delay).unwrap();
    // info!("Display cleared to white");

    let mut display = Display7in5::default();

    // make background white
    Rectangle::new(Point::new(0, 0), Size::new(width as u32, height as u32))
        .into_styled(PrimitiveStyle::with_fill(TriColor::White))
        .draw(&mut display)
        .unwrap();

    let bmp_data = include_bytes!("../../assets/frieren-1.bmp");
    let bmp: Bmp<BinaryColor> = Bmp::from_slice(bmp_data).unwrap();

    // Draw the image at position (0, 0) or adjust as needed
    Image::new(&bmp, Point::new(0, 0))
        .draw(&mut display.color_converted())
        .unwrap();

    epd.update_and_display_frame(&mut spi_device, &mut display.buffer(), &mut delay).unwrap();
    epd.wait_until_idle(&mut spi_device, &mut delay).unwrap();
    info!("Circle drawn");

    // Put display to sleep to prevent further refreshes
    epd.sleep(&mut spi_device, &mut delay).unwrap();
    info!("EPD set to sleep mode");

    loop {
        info!("Entering sleep for 5 seconds...");
        delay.delay_millis(5000);
        info!("Sleeping...");
    }

    // for inspiration have a look at the examples at https://github.com/esp-rs/esp-hal/tree/esp-hal-v~1.0/examples
}
