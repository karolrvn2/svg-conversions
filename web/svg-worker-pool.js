const DEFAULT_POOL_SIZE = Math.max(
  1,
  Math.min(4, globalThis.navigator?.hardwareConcurrency ?? 2),
);

export class SvgWorkerPool {
  #workers = [];
  #queue = [];
  #pending = new Map();
  #nextId = 0;
  #closed = false;

  constructor({ size = DEFAULT_POOL_SIZE, workerUrl = new URL("./svg-worker.js", import.meta.url) } = {}) {
    if (!Number.isInteger(size) || size < 1) {
      throw new TypeError("Worker pool size must be a positive integer");
    }
    for (let index = 0; index < size; index += 1) {
      const worker = new Worker(workerUrl, { type: "module" });
      const slot = { worker, busy: false };
      worker.addEventListener("message", ({ data }) => this.#settle(slot, data));
      worker.addEventListener("error", (event) => this.#failWorker(slot, event));
      this.#workers.push(slot);
    }
  }

  process(svg, options = {}) {
    if (typeof svg !== "string") {
      return Promise.reject(new TypeError("svg must be a string"));
    }
    return this.#enqueue("process", { svg, ...options });
  }

  generateColorMap(primaryColor = "#00acc1") {
    return this.#enqueue("colorMap", { primaryColor });
  }

  close() {
    if (this.#closed) return;
    this.#closed = true;
    const error = new Error("SVG worker pool was closed");
    for (const task of this.#queue.splice(0)) task.reject(error);
    for (const task of this.#pending.values()) task.reject(error);
    this.#pending.clear();
    for (const { worker } of this.#workers) worker.terminate();
  }

  #enqueue(operation, payload) {
    if (this.#closed) {
      return Promise.reject(new Error("SVG worker pool is closed"));
    }
    return new Promise((resolve, reject) => {
      this.#queue.push({ id: ++this.#nextId, operation, payload, resolve, reject });
      this.#dispatch();
    });
  }

  #dispatch() {
    for (const slot of this.#workers) {
      if (slot.busy || this.#queue.length === 0) continue;
      const task = this.#queue.shift();
      slot.busy = true;
      this.#pending.set(task.id, { ...task, slot });
      slot.worker.postMessage({ id: task.id, operation: task.operation, payload: task.payload });
    }
  }

  #settle(slot, { id, result, error }) {
    const task = this.#pending.get(id);
    if (!task) return;
    this.#pending.delete(id);
    slot.busy = false;
    if (error) task.reject(new Error(error));
    else task.resolve(result);
    this.#dispatch();
  }

  #failWorker(slot, event) {
    const task = [...this.#pending.values()].find((item) => item.slot === slot);
    if (task) {
      this.#pending.delete(task.id);
      task.reject(event.error ?? new Error(event.message || "SVG worker failed"));
    }
    slot.busy = false;
    this.#dispatch();
  }
}
