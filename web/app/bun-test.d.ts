declare module "bun:test" {
  export function describe(name: string, fn: () => void): void;
  export function test(name: string, fn: () => void | Promise<void>): void;
  export function expect<T>(actual: T): {
    toBe(expected: unknown): void;
    toEqual(expected: unknown): void;
    toContain(expected: unknown): void;
    toBeGreaterThanOrEqual(expected: number): void;
    not: {
      toContain(expected: unknown): void;
      toBe(expected: unknown): void;
    };
  };
}
