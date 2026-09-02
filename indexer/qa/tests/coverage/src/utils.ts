import crypto from 'crypto';
import path from 'path';

export const normalizeText = (value: string): string =>
  value
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, ' ')
    .replace(/\s+/g, ' ')
    .trim();

export const tokenize = (value: string): string[] => normalizeText(value).split(' ').filter(Boolean);

export const unique = <T>(values: T[]): T[] => Array.from(new Set(values));

export const toRelativeFromRepo = (repoRoot: string, filePath: string): string =>
  path.relative(repoRoot, filePath).replaceAll(path.sep, '/');

export const sha256 = (value: string): string =>
  crypto.createHash('sha256').update(value, 'utf8').digest('hex');

export const toKebabTokens = (value: string): string[] =>
  value
    .replace(/([a-z])([A-Z])/g, '$1 $2')
    .replace(/_/g, ' ')
    .split(/[^a-zA-Z0-9]+/)
    .map((t) => t.toLowerCase())
    .filter(Boolean);

export const toUtcCompact = (date: Date = new Date()): string => {
  const y = date.getUTCFullYear();
  const m = String(date.getUTCMonth() + 1).padStart(2, '0');
  const d = String(date.getUTCDate()).padStart(2, '0');
  const hh = String(date.getUTCHours()).padStart(2, '0');
  const mm = String(date.getUTCMinutes()).padStart(2, '0');
  const ss = String(date.getUTCSeconds()).padStart(2, '0');
  return `${y}${m}${d}${hh}${mm}${ss}`;
};
