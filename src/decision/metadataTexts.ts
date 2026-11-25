// Approved in T-24 (FR-05.4).

/** "Reference No 46 is also used by kim-2021.pdf and 2 more." */
export function sameRefNoMessage(refNo: number, others: string[]): string {
  const more = others.length > 1 ? ` and ${others.length - 1} more` : "";
  return `Reference No ${refNo} is also used by ${others[0]}${more}.`;
}
