import { JSDOM } from "jsdom";

/** Ensures dom environment before tests run. */
export function ensureDomEnvironment() {
  if (
    typeof globalThis.window === "undefined" ||
    typeof globalThis.document === "undefined"
  ) {
    const dom = new JSDOM("<!doctype html><html><body></body></html>", {
      url: "http://localhost",
    });
    const window = dom.window;

    Object.defineProperty(globalThis, "window", {
      value: window,
      configurable: true,
    });
    Object.defineProperty(globalThis, "self", {
      value: window,
      configurable: true,
    });
    Object.defineProperty(globalThis, "document", {
      value: window.document,
      configurable: true,
    });
    Object.defineProperty(globalThis, "navigator", {
      value: window.navigator,
      configurable: true,
    });

    for (const property of [
      "Blob",
      "CustomEvent",
      "Event",
      "File",
      "FileList",
      "FormData",
      "HTMLButtonElement",
      "HTMLFormElement",
      "HTMLElement",
      "HTMLInputElement",
      "HTMLSelectElement",
      "HTMLTextAreaElement",
      "InputEvent",
      "KeyboardEvent",
      "MouseEvent",
      "Node",
    ]) {
      installGlobal(property, window);
    }
  }

  installStorage("localStorage");
  installStorage("sessionStorage");
}

/** Handles install global behavior for this module. */
function installGlobal(property: string, window: Window) {
  const windowRecord = window as unknown as Record<string, unknown>;
  const value = windowRecord[property];
  if (value === undefined || property in globalThis) {
    return;
  }

  Object.defineProperty(globalThis, property, {
    value,
    configurable: true,
  });
}

/** Handles install storage behavior for this module. */
function installStorage(property: "localStorage" | "sessionStorage") {
  try {
    void globalThis.window[property].length;
    return;
  } catch {
    const storage = createMemoryStorage();
    Object.defineProperty(globalThis.window, property, {
      value: storage,
      configurable: true,
    });
  }
}

/** Creates memory storage through the API. */
function createMemoryStorage(): Storage {
  const values = new Map<string, string>();

  return {
    get length() {
      return values.size;
    },
    clear() {
      values.clear();
    },
    getItem(key: string) {
      return values.get(key) ?? null;
    },
    key(index: number) {
      return Array.from(values.keys())[index] ?? null;
    },
    removeItem(key: string) {
      values.delete(key);
    },
    setItem(key: string, value: string) {
      values.set(key, value);
    },
  };
}

ensureDomEnvironment();
