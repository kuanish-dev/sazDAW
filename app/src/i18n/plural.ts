// Number agreement for the `en.ts`/`ru.ts` dictionaries (separate from `index.ts` so the dictionaries don't form an import cycle).

/**
 * Word form for a number: `[one, other]` for English, `[one, two–four, five]` for Russian
 * (1, 21 → one; 3, 24 → few; 5, 11, 14 → many). Does not insert the number itself.
 */
export function plural(n: number, forms: readonly [string, string] | readonly [string, string, string]): string {
  const abs = Math.abs(n);
  if (forms.length === 2) return abs === 1 ? forms[0] : forms[1];
  const tens = abs % 100;
  const ones = abs % 10;
  // A fractional number in Russian takes the two–four form.
  if (!Number.isInteger(abs)) return forms[1];
  if (tens >= 11 && tens <= 14) return forms[2];
  if (ones === 1) return forms[0];
  if (ones >= 2 && ones <= 4) return forms[1];
  return forms[2];
}
