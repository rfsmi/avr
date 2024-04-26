use cc;

fn main() {
    let mut build = cc::Build::new();
    for file in ["BME280_SensorAPI/bme280.c"] {
        build.file(file);
    }
    build.compile("foo");
}
