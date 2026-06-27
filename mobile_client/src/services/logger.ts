/* eslint-disable no-console */

/**
 * Type-safe logger utility wrapping console methods to comply with no-console lint policies
 * in regular source files.
 */
export const logger = {
  log(message: string, ...args: unknown[]): void {
    console.log(message, ...args);
  },
  error(message: string, ...args: unknown[]): void {
    console.error(message, ...args);
  }
};
