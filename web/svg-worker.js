import init, { generate_color_map, process_svg, process_svg_with_palette } from "../pkg/svg_conversion_wasm.js";

const ready = init();

self.addEventListener("message", async ({ data }) => {
  const { id, operation = "process", payload = {} } = data ?? {};
  try {
    await ready;
    const convert = (icon) => payload.secondaryColor
      ? process_svg_with_palette(
        icon.svg ?? icon,
        payload.primaryColor ?? "#00acc1",
        payload.secondaryColor,
        payload.contrast ?? 1,
        payload.brightness ?? 0,
        payload.outputMode ?? "rgb",
      )
      : process_svg(
        icon.svg ?? icon,
        payload.primaryColor ?? "#00acc1",
        payload.tolerance ?? 0.2,
        payload.outputMode ?? "rgb",
      );
    const result = operation === "colorMap"
      ? generate_color_map(payload.primaryColor ?? "#00acc1")
      : operation === "processBatch"
        ? (() => {
          const startedAt = performance.now();
          const icons = payload.icons.map((icon) => ({
            name: icon.name,
            pack: icon.pack,
            src: `data:image/svg+xml;charset=utf-8,${encodeURIComponent(convert(icon))}`,
          }));
          return { icons, processingMs: performance.now() - startedAt };
        })()
      : payload.secondaryColor
        ? convert(payload.svg)
        : convert(payload.svg);
    self.postMessage({ id, result });
  } catch (error) {
    self.postMessage({
      id,
      error: error instanceof Error ? error.message : String(error),
    });
  }
});
