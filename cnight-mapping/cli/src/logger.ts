/**
 * Logger utility with environment-based controls
 *
 * In production, only error logs are shown.
 * In development, all logs are shown.
 *
 * Usage:
 * - logger.log('[Module]', 'Message') - Only in development
 * - logger.error('[Module]', 'Error message') - Always shown
 * - logger.warn('[Module]', 'Warning message') - Always shown
 */

const isDevelopment = process.env.NODE_ENV === 'development';

class Logger {
  /**
   * Log messages (development only)
   */
  log(...args: unknown[]): void {
    if (isDevelopment) {
      console.log(...args);
    }
  }

  /**
   * Error messages (always shown)
   */
  error(...args: unknown[]): void {
    console.error(...args);
  }

  /**
   * Warning messages (always shown)
   */
  warn(...args: unknown[]): void {
    console.warn(...args);
  }

  /**
   * Info messages (development only)
   */
  info(...args: unknown[]): void {
    if (isDevelopment) {
      console.info(...args);
    }
  }

  /**
   * Debug messages (development only, can be toggled separately)
   */
  debug(...args: unknown[]): void {
    if (isDevelopment && process.env.NEXT_PUBLIC_DEBUG === 'true') {
      console.debug(...args);
    }
  }
}

export const logger = new Logger();

