avr-objcopy -O ihex target/avr-attiny85/debug/avr.elf target/avr-attiny85/debug/avr.hex

avrdude -p t85 -P usb -u -c usbasp -U flash:w:avr.elf:a