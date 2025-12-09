import { type ClassValue, clsx } from "clsx";
import { twMerge } from "tailwind-merge";

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}

export function formatBytes(bytes: number, decimals = 2): string {
  if (bytes === 0) return "0 B";

  const k = 1024;
  const dm = decimals < 0 ? 0 : decimals;
  const sizes = ["B", "KB", "MB", "GB", "TB"];

  const i = Math.floor(Math.log(bytes) / Math.log(k));

  return parseFloat((bytes / Math.pow(k, i)).toFixed(dm)) + " " + sizes[i];
}

export function formatDuration(ms: number): string {
  if (ms < 1000) {
    return `${ms}ms`;
  }
  return `${(ms / 1000).toFixed(2)}s`;
}

export function getStatusClass(status: number | undefined): string {
  if (!status) return "status-pending";
  if (status >= 200 && status < 300) return "status-success";
  if (status >= 300 && status < 400) return "status-redirect";
  if (status >= 400 && status < 500) return "status-client-error";
  if (status >= 500) return "status-server-error";
  return "status-pending";
}

export function getMethodClass(method: string): string {
  const m = method.toLowerCase();
  switch (m) {
    case "get":
      return "method-get";
    case "post":
      return "method-post";
    case "put":
    case "patch":
      return "method-put";
    case "delete":
      return "method-delete";
    default:
      return "method-options";
  }
}

export function truncateUrl(url: string, maxLength = 50): string {
  if (url.length <= maxLength) return url;
  return url.substring(0, maxLength - 3) + "...";
}

export function parseContentType(contentType: string | undefined): string {
  if (!contentType) return "unknown";
  const type = contentType.split(";")[0].trim().toLowerCase();

  if (type.includes("json")) return "json";
  if (type.includes("html")) return "html";
  if (type.includes("xml")) return "xml";
  if (type.includes("javascript")) return "javascript";
  if (type.includes("css")) return "css";
  if (type.includes("image")) return "image";
  if (type.includes("text")) return "text";
  if (type.includes("form")) return "form";

  return type;
}
