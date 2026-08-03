use gloo_net::http::Request;
use gloo_timers::callback::Timeout;
use serde::{Deserialize, Serialize};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
use wasm_bindgen::{JsCast, JsValue, closure::Closure};
use wasm_bindgen_futures::spawn_local;
use web_sys::{
    Event, HtmlInputElement, HtmlTextAreaElement, MessageEvent, Worker, WorkerOptions, WorkerType,
};
use yew::prelude::*;

const SAMPLE: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 640 420"><rect width="640" height="420" rx="36" fill="#111827"/><circle cx="180" cy="190" r="112" fill="#5d6574"/><path d="M335 95h190a28 28 0 0 1 28 28v174a28 28 0 0 1-28 28H335z" fill="#d9dce2"/><path d="M372 151h144M372 202h108M372 253h126" fill="none" stroke="#282f3c" stroke-width="22" stroke-linecap="round"/></svg>"##;

#[derive(Clone, Deserialize, Serialize, PartialEq)]
struct SourceIcon {
    name: String,
    pack: String,
    svg: String,
}

#[derive(Clone, Deserialize, PartialEq)]
struct RenderedIcon {
    name: String,
    pack: String,
    src: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct BatchResult {
    icons: Vec<RenderedIcon>,
    processing_ms: f64,
}

#[derive(Clone, PartialEq)]
struct Metrics {
    catalog_ms: f64,
    conversion_ms: f64,
    worker_cpu_ms: f64,
    icon_count: usize,
    worker_count: usize,
}

struct SingleWorker {
    worker: Worker,
    next_id: Cell<u32>,
    _listener: Closure<dyn FnMut(MessageEvent)>,
}

impl SingleWorker {
    fn new(callback: Callback<Result<String, String>>) -> Result<Rc<Self>, JsValue> {
        let options = WorkerOptions::new();
        options.set_type(WorkerType::Module);
        let worker = Worker::new_with_options("/web/svg-worker.js", &options)?;
        let listener = Closure::wrap(Box::new(move |event: MessageEvent| {
            let data = event.data();
            if let Ok(error) = js_sys::Reflect::get(&data, &"error".into()) {
                if !error.is_undefined() {
                    callback.emit(Err(error
                        .as_string()
                        .unwrap_or_else(|| "Worker failed".into())));
                    return;
                }
            }
            if let Ok(result) = js_sys::Reflect::get(&data, &"result".into()) {
                callback.emit(Ok(result.as_string().unwrap_or_default()));
            }
        }) as Box<dyn FnMut(_)>);
        worker.set_onmessage(Some(listener.as_ref().unchecked_ref()));
        Ok(Rc::new(Self {
            worker,
            next_id: Cell::new(0),
            _listener: listener,
        }))
    }

    fn process(&self, svg: &str, primary: &str, secondary: &str, contrast: f64, brightness: f64) {
        let payload = to_js_object(&serde_json::json!({
            "svg": svg, "primaryColor": primary, "secondaryColor": secondary,
            "contrast": contrast, "brightness": brightness, "outputMode": "rgb"
        }));
        let id = self.next_id.get() + 1;
        self.next_id.set(id);
        let message = js_sys::Object::new();
        let _ = js_sys::Reflect::set(&message, &"id".into(), &id.into());
        let _ = js_sys::Reflect::set(&message, &"operation".into(), &"process".into());
        let _ = js_sys::Reflect::set(&message, &"payload".into(), &payload);
        let _ = self.worker.post_message(&message);
    }
}

impl Drop for SingleWorker {
    fn drop(&mut self) {
        self.worker.terminate();
    }
}

struct WorkerFarm {
    workers: Vec<Worker>,
    _listeners: Vec<Closure<dyn FnMut(MessageEvent)>>,
}

impl WorkerFarm {
    fn new(
        count: usize,
        callback: Callback<Result<BatchResult, String>>,
    ) -> Result<Rc<Self>, JsValue> {
        let mut workers = Vec::with_capacity(count);
        let mut listeners = Vec::with_capacity(count);
        for _ in 0..count {
            let options = WorkerOptions::new();
            options.set_type(WorkerType::Module);
            let worker = Worker::new_with_options("/web/svg-worker.js", &options)?;
            let callback = callback.clone();
            let listener = Closure::wrap(Box::new(move |event: MessageEvent| {
                let data = event.data();
                if let Ok(error) = js_sys::Reflect::get(&data, &"error".into()) {
                    if !error.is_undefined() {
                        callback.emit(Err(error
                            .as_string()
                            .unwrap_or_else(|| "Worker failed".into())));
                        return;
                    }
                }
                match js_sys::Reflect::get(&data, &"result".into())
                    .ok()
                    .and_then(|value| serde_wasm_bindgen::from_value(value).ok())
                {
                    Some(result) => callback.emit(Ok(result)),
                    None => callback.emit(Err("Invalid batch result from worker".into())),
                }
            }) as Box<dyn FnMut(_)>);
            worker.set_onmessage(Some(listener.as_ref().unchecked_ref()));
            workers.push(worker);
            listeners.push(listener);
        }
        Ok(Rc::new(Self {
            workers,
            _listeners: listeners,
        }))
    }

    fn process(
        &self,
        icons: &[SourceIcon],
        primary: &str,
        secondary: &str,
        contrast: f64,
        brightness: f64,
    ) {
        let chunk_size = icons.len().div_ceil(self.workers.len());
        for (index, (worker, chunk)) in self
            .workers
            .iter()
            .zip(icons.chunks(chunk_size))
            .enumerate()
        {
            let payload = to_js_object(&serde_json::json!({
                "icons": chunk, "primaryColor": primary, "secondaryColor": secondary,
                "contrast": contrast, "brightness": brightness, "outputMode": "rgb"
            }));
            let message = js_sys::Object::new();
            let _ = js_sys::Reflect::set(&message, &"id".into(), &(index as u32).into());
            let _ = js_sys::Reflect::set(&message, &"operation".into(), &"processBatch".into());
            let _ = js_sys::Reflect::set(&message, &"payload".into(), &payload);
            let _ = worker.post_message(&message);
        }
    }
}

impl Drop for WorkerFarm {
    fn drop(&mut self) {
        for worker in &self.workers {
            worker.terminate();
        }
    }
}

fn now() -> f64 {
    web_sys::window()
        .and_then(|w| w.performance())
        .map(|p| p.now())
        .unwrap_or_default()
}

fn to_js_object<T: Serialize + ?Sized>(value: &T) -> JsValue {
    let serializer = serde_wasm_bindgen::Serializer::new().serialize_maps_as_objects(true);
    value.serialize(&serializer).unwrap_or(JsValue::NULL)
}

#[function_component(App)]
fn app() -> Html {
    let source = use_state(|| SAMPLE.to_owned());
    let output = use_state(String::new);
    let error = use_state(|| None::<String>);
    let primary = use_state(|| "#ff6b35".to_owned());
    let secondary = use_state(|| "#5b8cff".to_owned());
    let contrast = use_state(|| 1.0_f64);
    let brightness = use_state(|| 0.0_f64);
    let working = use_state(|| true);
    let catalog = use_state(|| None::<Rc<Vec<SourceIcon>>>);
    let catalog_ms = use_state(|| 0.0_f64);
    let catalog_loading = use_state(|| true);
    let gallery = use_state(|| Rc::new(Vec::<RenderedIcon>::new()));
    let benchmark_running = use_state(|| false);
    let metrics = use_state(|| None::<Metrics>);
    let active_farm = use_state(|| None::<Rc<WorkerFarm>>);

    {
        let catalog = catalog.clone();
        let catalog_ms = catalog_ms.clone();
        let catalog_loading = catalog_loading.clone();
        let error = error.clone();
        use_effect_with((), move |_| {
            spawn_local(async move {
                let started = now();
                match Request::get("/icons/catalog.json").send().await {
                    Ok(response) => match response.json::<Vec<SourceIcon>>().await {
                        Ok(icons) => {
                            catalog_ms.set(now() - started);
                            catalog.set(Some(Rc::new(icons)));
                        }
                        Err(err) => error.set(Some(format!("Could not parse icon catalog: {err}"))),
                    },
                    Err(err) => error.set(Some(format!("Could not load icon catalog: {err}"))),
                }
                catalog_loading.set(false);
            });
            || ()
        });
    }

    let single = {
        let output = output.clone();
        let error = error.clone();
        let working = working.clone();
        use_state(move || {
            SingleWorker::new(Callback::from(move |result| {
                working.set(false);
                match result {
                    Ok(svg) => {
                        output.set(svg);
                        error.set(None);
                    }
                    Err(message) => error.set(Some(message)),
                }
            }))
            .ok()
        })
    };

    {
        let dependencies = (
            (*source).clone(),
            (*primary).clone(),
            (*secondary).clone(),
            *contrast,
            *brightness,
        );
        let single = single.clone();
        let working = working.clone();
        use_effect_with(
            dependencies,
            move |(svg, primary, secondary, contrast, brightness)| {
                let single = single.clone();
                let working = working.clone();
                let (svg, primary, secondary, contrast, brightness) = (
                    svg.clone(),
                    primary.clone(),
                    secondary.clone(),
                    *contrast,
                    *brightness,
                );
                let timeout = Timeout::new(100, move || {
                    if let Some(worker) = &*single {
                        working.set(true);
                        worker.process(&svg, &primary, &secondary, contrast, brightness);
                    }
                });
                move || drop(timeout)
            },
        );
    }

    let run_benchmark = {
        let catalog = catalog.clone();
        let gallery = gallery.clone();
        let benchmark_running = benchmark_running.clone();
        let metrics = metrics.clone();
        let active_farm = active_farm.clone();
        let error = error.clone();
        let primary = primary.clone();
        let secondary = secondary.clone();
        let contrast = contrast.clone();
        let brightness = brightness.clone();
        let catalog_ms_value = *catalog_ms;
        Callback::from(move |_| {
            let Some(icons) = (*catalog).clone() else {
                return;
            };
            gallery.set(Rc::new(Vec::new()));
            metrics.set(None);
            benchmark_running.set(true);
            error.set(None);
            let worker_count = web_sys::window()
                .map(|w| w.navigator().hardware_concurrency() as usize)
                .unwrap_or(2)
                .clamp(1, 4)
                .min(icons.len());
            let started = now();
            let accumulator = Rc::new(RefCell::new((
                0usize,
                0.0f64,
                Vec::<RenderedIcon>::with_capacity(icons.len()),
            )));
            let callback = {
                let accumulator = accumulator.clone();
                let gallery = gallery.clone();
                let metrics = metrics.clone();
                let benchmark_running = benchmark_running.clone();
                let active_farm = active_farm.clone();
                let error = error.clone();
                Callback::from(move |result: Result<BatchResult, String>| match result {
                    Err(message) => {
                        error.set(Some(message));
                        benchmark_running.set(false);
                        active_farm.set(None);
                    }
                    Ok(mut batch) => {
                        let mut state = accumulator.borrow_mut();
                        state.0 += 1;
                        state.1 += batch.processing_ms;
                        state.2.append(&mut batch.icons);
                        if state.0 == worker_count {
                            state
                                .2
                                .sort_by(|a, b| a.pack.cmp(&b.pack).then(a.name.cmp(&b.name)));
                            let rendered = std::mem::take(&mut state.2);
                            let count = rendered.len();
                            gallery.set(Rc::new(rendered));
                            metrics.set(Some(Metrics {
                                catalog_ms: catalog_ms_value,
                                conversion_ms: now() - started,
                                worker_cpu_ms: state.1,
                                icon_count: count,
                                worker_count,
                            }));
                            benchmark_running.set(false);
                            active_farm.set(None);
                        }
                    }
                })
            };
            match WorkerFarm::new(worker_count, callback) {
                Ok(farm) => {
                    farm.process(&icons, &primary, &secondary, *contrast, *brightness);
                    active_farm.set(Some(farm));
                }
                Err(_) => {
                    error.set(Some("Could not start benchmark workers".into()));
                    benchmark_running.set(false);
                }
            }
        })
    };

    let on_source = {
        let source = source.clone();
        Callback::from(move |event: InputEvent| {
            source.set(event.target_unchecked_into::<HtmlTextAreaElement>().value())
        })
    };
    let color_handler = |setter: UseStateHandle<String>| {
        Callback::from(move |event: Event| {
            setter.set(event.target_unchecked_into::<HtmlInputElement>().value())
        })
    };
    let range_handler = |setter: UseStateHandle<f64>| {
        Callback::from(move |event: InputEvent| {
            if let Ok(value) = event
                .target_unchecked_into::<HtmlInputElement>()
                .value()
                .parse()
            {
                setter.set(value);
            }
        })
    };

    html! {
      <>
        <header class="masthead"><p class="eyebrow">{"33,166 icons · Rust · Yew · WebAssembly · Web Workers"}</p><h1>{"SVG Palette Lab"}</h1><p>{"Re-map one SVG—or an entire open-source icon catalog—without blocking the main thread."}</p></header>
        <main class="workspace">
          <section class="panel controls" aria-labelledby="controls-title">
            <div class="section-heading"><h2 id="controls-title">{"Palette"}</h2><span class="status" aria-live="polite">{if *working { "Converting…" } else { "Ready" }}</span></div>
            <div class="color-grid">
              <label><span>{"Primary · shadows"}</span><span class="color-control"><input type="color" value={(*primary).clone()} onchange={color_handler(primary.clone())}/><output>{&*primary}</output></span></label>
              <label><span>{"Secondary · highlights"}</span><span class="color-control"><input type="color" value={(*secondary).clone()} onchange={color_handler(secondary.clone())}/><output>{&*secondary}</output></span></label>
            </div>
            <label class="range"><span><b>{"Contrast"}</b><output>{format!("{:.0}%", *contrast * 100.0)}</output></span><input type="range" min="0" max="2" step="0.01" value={contrast.to_string()} oninput={range_handler(contrast.clone())}/></label>
            <label class="range"><span><b>{"Brightness"}</b><output>{format!("{:+.0}%", *brightness * 100.0)}</output></span><input type="range" min="-1" max="1" step="0.01" value={brightness.to_string()} oninput={range_handler(brightness.clone())}/></label>
            <label class="source-label" for="svg-source"><span><b>{"SVG source"}</b><small>{"Paste any SVG markup"}</small></span></label>
            <textarea id="svg-source" spellcheck="false" value={(*source).clone()} oninput={on_source}/>
          </section>
          <section class="panel preview" aria-labelledby="preview-title">
            <div class="section-heading"><h2 id="preview-title">{"Preview"}</h2><span class="palette-chip" style={format!("--primary:{};--secondary:{}", *primary, *secondary)} aria-hidden="true"></span></div>
            if let Some(message) = &*error { <p class="error" role="alert">{message}</p> }
            <div class="preview-stage"><iframe title="Converted SVG preview" sandbox="" srcdoc={(*output).clone()}></iframe></div>
            <p class="preview-note">{"The preview is sandboxed. SVG markup never enters the demo page DOM."}</p>
          </section>
        </main>
        <section class="benchmark" aria-labelledby="benchmark-title">
          <div class="benchmark-intro">
            <div><p class="eyebrow">{"Worker benchmark"}</p><h2 id="benchmark-title">{"Colorize the complete svg-icons catalog"}</h2><p>{"37 packs from svg-icons/svg-icons and svglogos.dev. Conversion is split across up to four module workers."}</p></div>
            <button type="button" onclick={run_benchmark} disabled={*catalog_loading || *benchmark_running || catalog.is_none()}>{if *catalog_loading { "Loading 33,166 icons…" } else if *benchmark_running { "Workers are running…" } else { "Run all 33,166 icons" }}</button>
          </div>
          if let Some(stats) = &*metrics {
            <dl class="metrics" aria-label="Benchmark results">
              <div><dt>{"Total icons"}</dt><dd>{stats.icon_count}</dd></div>
              <div><dt>{"Catalog load"}</dt><dd>{format!("{:.2} s", stats.catalog_ms / 1000.0)}</dd></div>
              <div><dt>{"Conversion wall time"}</dt><dd>{format!("{:.2} s", stats.conversion_ms / 1000.0)}</dd></div>
              <div><dt>{"Average per icon"}</dt><dd>{format!("{:.3} ms", stats.conversion_ms / stats.icon_count as f64)}</dd></div>
              <div><dt>{"Throughput"}</dt><dd>{format!("{:.0} icons/s", stats.icon_count as f64 / (stats.conversion_ms / 1000.0))}</dd></div>
              <div><dt>{"Workers / CPU time"}</dt><dd>{format!("{} / {:.2} s", stats.worker_count, stats.worker_cpu_ms / 1000.0)}</dd></div>
            </dl>
          }
          <div class="icon-grid" aria-label="Converted icon gallery">
            {for gallery.iter().enumerate().map(|(index, icon)| html! {
              <figure class={classes!("icon-card", (index >= 100).then_some("deferred"))}>
                <img src={icon.src.clone()} alt="" width="56" height="56" loading={(index >= 100).then_some("lazy")}/>
                <figcaption><b>{&icon.name}</b><span>{&icon.pack}</span></figcaption>
              </figure>
            })}
          </div>
        </section>
      </>
    }
}

fn main() {
    yew::Renderer::<App>::new().render();
}
