# SVG conversion (Rust + Web Workers)

This package colorizes SVG documents in Rust compiled to WebAssembly. The Wasm
module runs only inside module Web Workers, keeping SVG parsing and conversion
off the browser's main thread.

The converter uses `xmltree` as an owned SVG/XML DOM. It updates SVG paint
presentation attributes and equivalent declarations in inline `style`
attributes. `none`, `transparent`, `url(...)`, and existing `var(...)` paints
are preserved.

## Filesystem CLI

The native Rust CLI is the replacement for the previous TypeScript `npm start`
program. It recursively reads SVG files from the input directory and writes
their converted counterparts to the output directory, preserving every nested
relative path. The paths are used exactly as supplied, so relative paths remain
relative to the current directory.

```sh
npm start -- ./example_data ./out "#00acc1" 0.3 rgb
# Equivalent without npm:
cargo run --release -- ./example_data ./out "#00acc1" 0.3 rgb
```

Arguments:

| argument | required | default |
| --- | --- | --- |
| `input_dir` | yes | |
| `output_dir` | yes | |
| `primary_color` | no | `#BBBBBB` |
| `tolerance` | no | `0.3` |
| `output_mode` | no | `rgb` (`css_vars` also writes `color_map.sass`) |

`input_dir` and `output_dir` must differ. This native command is independent
of the WebAssembly build and does not modify `pkg/`, `demo/`, or the worker
files.

## WebAssembly build and demo

Install Rust, the `wasm32-unknown-unknown` target, and `wasm-pack`, then run:

```sh
npm run build
npm test
```

Run the interactive Yew demo during development with:

```sh
npm run start:demo
```

Create a deployable demo in `dist/` with `npm run build:demo`.

## Full icon benchmark

The demo can prepare and colorize all 33,166 icons from the 37 packs in
[`svg-icons/svg-icons`](https://github.com/svg-icons/svg-icons) plus the
[`svglogos.dev`](https://svglogos.dev) brand logos (CC0, from
[`gilbarbara/logos`](https://github.com/gilbarbara/logos)). The package
versions and source commits are pinned by `scripts/prepare-icons.mjs`.

```sh
npm run prepare:icons
```

The generated catalog is cached locally and excluded from Git. In the demo,
select **Run all 33,166 icons** to distribute the conversion across up to four
Web Workers. Results include catalog load time, conversion wall time, average
time per icon, icons per second, and cumulative worker CPU time.

Serve the package over HTTP(S); browsers do not reliably load module workers or
Wasm from `file://` URLs.

## Frontend API

```js
import { SvgWorkerPool } from "./web/svg-worker-pool.js";

const converters = new SvgWorkerPool();

const converted = await converters.process(svgText, {
  primaryColor: "#00acc1",
  tolerance: 0.2,
  outputMode: "rgb", // or "css_vars"
});

const sassVariables = await converters.generateColorMap("#00acc1");

// Call when the owning page/component is disposed.
converters.close();
```

The default pool size is `min(4, navigator.hardwareConcurrency)`. Set a custom
size with `new SvgWorkerPool({ size: 2 })`. Each request returns a Promise and
queued conversions are distributed across idle workers.

## Conversion behavior

- A missing root `fill` gets the SVG default `rgb(0,0,0)` before conversion.
- Source colors are converted to grayscale using the original weighted formula.
- Their lightness range is mapped around the primary color using `tolerance`.
- Output lightness is normalized to one of 256 levels.
- One-color SVGs resolve directly to the selected primary color.
- Invalid SVG, primary color, tolerance, or output mode rejects the Promise.

XML serialization may normalize whitespace, quoting, and empty-element syntax;
the SVG structure and semantics are retained.
