/** Value for aria-describedby of a control inside Field.svelte. */
export function describedBy(id: string, hint?: string, error?: string): string | undefined {
  const ids = [error && `${id}-error`, hint && `${id}-hint`].filter(Boolean);
  return ids.length > 0 ? ids.join(" ") : undefined;
}
