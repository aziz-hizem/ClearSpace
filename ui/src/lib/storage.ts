/** localStorage for small UI preferences only. Never throws: storage can be unavailable. */
export function load<T>(key: string, fallback: T, valid?: (v: unknown) => v is T): T {
  try {
    const raw = localStorage.getItem(`cs.${key}`);
    if (raw === null) return fallback;
    const v: unknown = JSON.parse(raw);
    return valid ? (valid(v) ? v : fallback) : (v as T);
  } catch {
    return fallback;
  }
}

export function save(key: string, value: unknown) {
  try {
    localStorage.setItem(`cs.${key}`, JSON.stringify(value));
  } catch {
    // Preferences are a convenience; ignore.
  }
}

export const oneOf =
  <T extends string>(...values: T[]) =>
  (v: unknown): v is T =>
    typeof v === "string" && (values as string[]).includes(v);
