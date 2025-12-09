/**
 * Safely serialize a value for transmission
 * Handles circular references and special types
 */
export function serialize(value: unknown, seen = new WeakSet()): unknown {
  // Handle primitives
  if (value === null) return null;
  if (value === undefined) return { __type: "undefined" };
  if (typeof value === "boolean") return value;
  if (typeof value === "number") {
    if (Number.isNaN(value)) return { __type: "NaN" };
    if (!Number.isFinite(value)) return { __type: value > 0 ? "Infinity" : "-Infinity" };
    return value;
  }
  if (typeof value === "string") return value;
  if (typeof value === "bigint") return { __type: "bigint", value: value.toString() };
  if (typeof value === "symbol") return { __type: "symbol", description: value.description };
  if (typeof value === "function") {
    return {
      __type: "function",
      name: value.name || "(anonymous)",
      preview: value.toString().slice(0, 200),
    };
  }

  // Handle objects
  if (typeof value === "object") {
    // Check for circular references
    if (seen.has(value)) {
      return { __type: "circular" };
    }
    seen.add(value);

    // Handle special types
    if (value instanceof Error) {
      return {
        __type: "error",
        name: value.name,
        message: value.message,
        stack: value.stack,
      };
    }

    if (value instanceof Date) {
      return { __type: "date", value: value.toISOString() };
    }

    if (value instanceof RegExp) {
      return { __type: "regexp", source: value.source, flags: value.flags };
    }

    if (value instanceof Map) {
      return {
        __type: "map",
        entries: Array.from(value.entries()).map(([k, v]) => [
          serialize(k, seen),
          serialize(v, seen),
        ]),
      };
    }

    if (value instanceof Set) {
      return {
        __type: "set",
        values: Array.from(value).map((v) => serialize(v, seen)),
      };
    }

    if (value instanceof WeakMap) {
      return { __type: "weakmap" };
    }

    if (value instanceof WeakSet) {
      return { __type: "weakset" };
    }

    if (typeof ArrayBuffer !== "undefined" && value instanceof ArrayBuffer) {
      return {
        __type: "arraybuffer",
        byteLength: value.byteLength,
      };
    }

    if (typeof SharedArrayBuffer !== "undefined" && value instanceof SharedArrayBuffer) {
      return {
        __type: "sharedarraybuffer",
        byteLength: value.byteLength,
      };
    }

    if (ArrayBuffer.isView(value)) {
      return {
        __type: "typedarray",
        name: value.constructor.name,
        length: (value as unknown as { length: number }).length,
        preview: Array.from(
          (value as unknown as ArrayLike<number>).slice
            ? (value as unknown as { slice(start: number, end: number): ArrayLike<number> }).slice(0, 10)
            : []
        ),
      };
    }

    if (typeof HTMLElement !== "undefined" && value instanceof HTMLElement) {
      return {
        __type: "element",
        tagName: value.tagName.toLowerCase(),
        id: value.id || undefined,
        className: value.className || undefined,
        outerHTML: value.outerHTML.slice(0, 500),
      };
    }

    if (typeof Node !== "undefined" && value instanceof Node) {
      return {
        __type: "node",
        nodeType: value.nodeType,
        nodeName: value.nodeName,
      };
    }

    if (typeof Window !== "undefined" && value instanceof Window) {
      return { __type: "window" };
    }

    if (typeof Document !== "undefined" && value instanceof Document) {
      return { __type: "document", title: value.title };
    }

    if (value instanceof Promise) {
      return { __type: "promise" };
    }

    // Handle arrays
    if (Array.isArray(value)) {
      return value.map((item) => serialize(item, seen));
    }

    // Handle plain objects
    try {
      const result: Record<string, unknown> = {};
      const keys = Object.keys(value);

      // Limit number of properties
      const maxKeys = 100;
      for (let i = 0; i < Math.min(keys.length, maxKeys); i++) {
        const key = keys[i];
        result[key] = serialize((value as Record<string, unknown>)[key], seen);
      }

      if (keys.length > maxKeys) {
        result.__truncated = `${keys.length - maxKeys} more properties`;
      }

      return result;
    } catch {
      return { __type: "object", preview: String(value) };
    }
  }

  return { __type: "unknown", preview: String(value) };
}
