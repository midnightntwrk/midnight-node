import type { Facet } from './types.ts';
import { normalizeText, unique } from './utils.ts';

export interface FacetKeywordConfig {
  negative: string[];
  schemaValidation: string[];
  edgeCase: string[];
  streaming: string[];
  positive: string[];
}

const hasKeyword = (text: string, keywords: string[]): boolean => {
  const normalized = normalizeText(text);
  return keywords.some((k) => normalized.includes(normalizeText(k)));
};

export const classifyFacets = (
  textEvidence: string,
  labels: string[],
  config: FacetKeywordConfig,
): Facet[] => {
  const joined = `${textEvidence} ${labels.join(' ')}`;
  const facets: Facet[] = [];

  if (hasKeyword(joined, config.negative)) facets.push('negative');
  if (hasKeyword(joined, config.schemaValidation)) facets.push('schemaValidation');
  if (hasKeyword(joined, config.edgeCase)) facets.push('edgeCase');
  if (hasKeyword(joined, config.streaming)) facets.push('streaming');
  if (hasKeyword(joined, config.positive) || facets.length === 0) facets.push('positive');

  return unique(facets);
};
