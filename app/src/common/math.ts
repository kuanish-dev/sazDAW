// Small arithmetic helpers shared by every role, the core and the mock.

export const clamp = (value: number, min: number, max: number): number => Math.min(max, Math.max(min, value));
