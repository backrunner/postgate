/**
 * Get a filtered stack trace, removing internal PostGate frames
 */
export function getStackTrace(): string {
  const err = new Error();
  const stack = err.stack || "";

  // Filter out PostGate internal frames
  const lines = stack.split("\n");
  const filteredLines = lines.filter((line) => {
    // Skip the Error line
    if (line.trim().startsWith("Error")) return false;
    // Skip PostGate internal frames
    if (line.includes("PostGate") || line.includes("postgate")) return false;
    // Skip console capture frames
    if (line.includes("ConsoleCapture") || line.includes("captureMethod")) return false;
    // Skip getStackTrace itself
    if (line.includes("getStackTrace")) return false;
    return true;
  });

  return filteredLines.join("\n").trim();
}

/**
 * Parse a stack trace into structured frames
 */
export interface StackFrame {
  functionName?: string;
  fileName?: string;
  lineNumber?: number;
  columnNumber?: number;
}

export function parseStackTrace(stack: string): StackFrame[] {
  const frames: StackFrame[] = [];
  const lines = stack.split("\n");

  for (const line of lines) {
    const frame = parseStackFrame(line);
    if (frame) {
      frames.push(frame);
    }
  }

  return frames;
}

function parseStackFrame(line: string): StackFrame | null {
  // Chrome/V8 format: "    at functionName (fileName:line:column)"
  const chromeMatch = line.match(/^\s*at\s+(?:(.+?)\s+\()?(.+):(\d+):(\d+)\)?$/);
  if (chromeMatch) {
    return {
      functionName: chromeMatch[1] || undefined,
      fileName: chromeMatch[2],
      lineNumber: parseInt(chromeMatch[3], 10),
      columnNumber: parseInt(chromeMatch[4], 10),
    };
  }

  // Firefox format: "functionName@fileName:line:column"
  const firefoxMatch = line.match(/^(.+)@(.+):(\d+):(\d+)$/);
  if (firefoxMatch) {
    return {
      functionName: firefoxMatch[1] || undefined,
      fileName: firefoxMatch[2],
      lineNumber: parseInt(firefoxMatch[3], 10),
      columnNumber: parseInt(firefoxMatch[4], 10),
    };
  }

  // Safari format: "functionName@fileName:line"
  const safariMatch = line.match(/^(.+)@(.+):(\d+)$/);
  if (safariMatch) {
    return {
      functionName: safariMatch[1] || undefined,
      fileName: safariMatch[2],
      lineNumber: parseInt(safariMatch[3], 10),
    };
  }

  return null;
}
