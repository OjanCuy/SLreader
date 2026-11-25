// Queue of temporary notifications (O-11). ToastRegion.svelte shows it.

export type ToastTone = "neutral" | "success" | "error";

export interface ToastAction {
  label: string;
  /** Runs after the toast has closed. */
  onselect: () => void;
}

export interface ToastData {
  id: number;
  tone: ToastTone;
  message: string;
  actions?: ToastAction[];
}

/**
 * Neutral and success toasts disappear after this long. Errors stay until closed, and a
 * toast with actions stays until one of them is chosen.
 */
export const TOAST_DURATION_MS = 4000;

let nextId = 0;

export const toasts = $state<ToastData[]>([]);

export function showToast(
  message: string,
  tone: ToastTone = "neutral",
  actions?: ToastAction[],
): number {
  const id = ++nextId;
  toasts.push({ id, tone, message, actions });
  if (tone !== "error" && !actions) setTimeout(() => dismissToast(id), TOAST_DURATION_MS);
  return id;
}

export function dismissToast(id: number): void {
  const index = toasts.findIndex((toast) => toast.id === id);
  if (index !== -1) toasts.splice(index, 1);
}
