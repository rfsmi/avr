use std::{env, path::PathBuf};

use cc;

fn main() {
    cc::Build::new()
        .no_default_flags(true)
        .file("BME280_SensorAPI/bme280.c")
        .compile("bme280");

    let bindings = bindgen::builder()
        .use_core()
        .clang_arg("--target=avr-unknown-unknown")
        .clang_arg("-ffreestanding")
        .clang_arg("-nostdlib")
        .size_t_is_usize(false)
        // Ensure we rebuild when header changes
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .clang_arg("-mmcu=attiny85")
        .header("BME280_SensorAPI/bme280.h")
        .generate()
        .unwrap();

    // Write the bindings to the $OUT_DIR/bindings.rs file.
    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("Couldn't write bindings!");
}
