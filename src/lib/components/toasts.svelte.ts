// Queue of temporary notifications (O-11). ToastRegion.svelte shows it.

export type ToastTone = "neutral" | "success" | "error";

export interface ToastData {
  id: number;
  tone: ToastTone;
  message: string;
}

/** Neutral and success toasts disappear after this long. Errors stay until closed. */
export const TOAST_DURATION_MS = 4000;

let nextId = 0;

export const toasts = $state<ToastData[]>([]);

export function showToast(message: string, tone: ToastTone = "neutral"): number {
  const id = ++nextId;
  toasts.push({ id, tone, message });
  if (tone !== "error") setTimeout(() => dismissToast(id), TOAST_DURATION_MS);
  return id;
}

export function dismissToast(id: number): void {
  const index = toasts.findIndex((toast) => toast.id === id);
  if (index !== -1) toasts.splice(index, 1);
}
