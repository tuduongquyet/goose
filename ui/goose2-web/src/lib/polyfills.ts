/**
 * Polyfills for web environment — call before app renders.
 *
 * - crypto.randomUUID: not available in non-secure contexts (HTTP).
 */

export function installPolyfills(): void {
  // crypto.randomUUID polyfill for HTTP (non-secure) contexts
  if (typeof crypto !== "undefined" && !crypto.randomUUID) {
    crypto.randomUUID = () =>
      "xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx".replace(/[xy]/g, (c) => {
        const r = (Math.random() * 16) | 0;
        const v = c === "x" ? r : (r & 0x3) | 0x8;
        return v.toString(16);
      }) as `${string}-${string}-${string}-${string}-${string}`;
  }
}
