# Helion
Ambilight clone for windows written in Rust, meant to be used with an Arduino running [LEDstream](https://github.com/adafruit/Adalight/tree/master/Arduino/LEDstream).

Using the [DXGCap](https://github.com/bryal/DXGCap) library to capture the screen in Windows, this program analyzes the image and sends resulting LED data to the Arduino.

[Test video](https://www.youtube.com/watch?v=3ZARz9ELfA4&feature=youtu.be)


# Config
Helion uses configuration generated by [HyperCon](https://github.com/tvdzwan/hyperion/wiki/configuration).
Config must be located in the current working directory, which is same as where the .exe is located when not run from command line.

Notes about config sections:

* `Device`:
	* `Output`: Serial port to use, e.g. "COM2" on windows.

	* `Baudrate`: Rate to use when sending pixel buffer to Arduino, LEDstream expects this to be 115200.

	* `Type`: Not read, but must be one where `Output` and `Baudrate` fields exist.

* `Construction`: Led placement. Everything is read.

* `Image Process`: Led capture areas. Blackborder stuff not read.

* `Frame Grabber`:
	* `Width` and `Height`, **REQUIRED**: Determines to what resolution frame is resized when analyzing colors, smaller is faster. Works best if dimensions are divisors of the native resolution. If a field is 0, native resolution is used in that dimension.

	* `Interval`: How often to capture the frame. If no smoothing is enabled, this also decides LED refresh rate. FPS = 1/interval.

* `Smoothing`:
	* `Type`: Type of smoothing to use, currently only `Linear smoothing`, no direct plans to add anything else.

	* `Time [ms]`: The time constant to use when smoothing. Larger values gives slower transition.

		* `Linear Smoothing`: `"previous value" + ("value difference" * max("Frame time difference" / "Time constant", 1))`

	* `Update Freq. [Hz]`: How often to update LEDs. Should be higher than FPS of `Frame Grabber` -> `Interval`. When no new frame has been captured, just keep smoothing the colors to previous frame.


* `Colors`: Everything is read.

* `External`: There are no plans to add support for anything under this tab. The stuff here is really just for Raspberry Pis with XBMC and stuff.


# Building
Dependencies:
```
Helion
|-- DXGCap
|-- serial-rust
|   |-- serial-C
```

Only Windows is supported at this time, as DXGCap is Windows only and I have not yet added support for any linux screen capturing.

## Windows

1. Build DXGCap and serial-C as dlls
2. Place DXGCap.dll and serial_c.dll in project root.
3. `cargo build --release`
4. ???
5. Profit

Note: While static linking would've been preferred, static libs, Windows, Rust, and x64 does not go nicely together. If you do succed building using static libs, please tell me how you did it!