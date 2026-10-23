export function basename(path: string): string {
  const parts = path.split("/");
  return parts[parts.length - 1] ?? path;
}

export function joinPath(dir: string, name: string): string {
  return dir.endsWith("/") ? dir + name : `${dir}/${name}`;
}

export function parentDir(path: string): string {
  const trimmed = path.replace(/\/+$/, "");
  const index = trimmed.lastIndexOf("/");
  return index <= 0 ? "/" : trimmed.slice(0, index);
}

/** Path with the given extension (`ext` includes the dot, e.g. `.wav`), appended if missing. */
export function withExtension(path: string, ext: string): string {
  return path.toLowerCase().endsWith(ext.toLowerCase()) ? path : `${path}${ext}`;
}

/** Parameter number for a label: no trailing zeros. */
export function formatParam(value: number): string {
  const abs = Math.abs(value);
  if (abs >= 1000) return value.toFixed(0);
  if (abs >= 100) return value.toFixed(1);
  if (abs >= 10) return value.toFixed(2);
  return value.toFixed(3).replace(/0+$/, "").replace(/\.$/, "");
}

/** Project file for the "folder/name" chosen in the dialog: `<folder>/<name>/<name>.dawproj`. */
export function projectFileFor(chosen: string): string {
  const folder = chosen.replace(/\/+$/, "").replace(/\.dawproj$/, "");
  return joinPath(folder, `${basename(folder)}.dawproj`);
}
