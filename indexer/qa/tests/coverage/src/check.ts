import path from 'path';
import type { CoverageReport, CriticalFieldRule } from './types.ts';
import { pathExists, readJson } from './io.ts';

interface CheckThresholdsConfig {
  enabled: boolean;
  maxOverallCoverageDropPercent: number;
  failOnNewMissingCritical: boolean;
  failOnCriticalRequirements: boolean;
  baselineFile: string;
}

interface CriticalConfig {
  criticalFields: CriticalFieldRule[];
}

export interface CheckOptions {
  repoRoot: string;
  outputDir: string;
  checkThresholdsPath: string;
  criticalOperationsPath: string;
}

const fieldKey = (rootType: string, field: string): string => `${rootType}.${field}`;

export const runCoverageChecks = async (options: CheckOptions): Promise<void> => {
  const latestRunPath = path.join(options.outputDir, 'latest-run.json');
  const hasLatestRun = await pathExists(latestRunPath);
  let reportPath = path.join(options.outputDir, 'coverage.json');
  if (hasLatestRun) {
    const latest = await readJson<{ outputDir: string }>(latestRunPath);
    reportPath = path.join(options.repoRoot, latest.outputDir, 'coverage.json');
    if (!(await pathExists(reportPath))) {
      throw new Error(
        `latest-run.json points at ${reportPath} which doesn't exist - re-run coverage:generate.`,
      );
    }
  } else if (!(await pathExists(reportPath))) {
    throw new Error(`coverage.json not found at ${reportPath}. Run coverage generation first.`);
  }

  const report = await readJson<CoverageReport>(reportPath);
  const thresholds = await readJson<CheckThresholdsConfig>(options.checkThresholdsPath);
  const critical = await readJson<CriticalConfig>(options.criticalOperationsPath);
  if (!thresholds.enabled) return;

  const failures: string[] = [];

  const baselinePath = path.isAbsolute(thresholds.baselineFile)
    ? thresholds.baselineFile
    : path.join(options.repoRoot, thresholds.baselineFile);
  if (await pathExists(baselinePath)) {
    const baseline = await readJson<CoverageReport>(baselinePath);
    const drop = baseline.summary.percentCovered - report.summary.percentCovered;
    if (drop > thresholds.maxOverallCoverageDropPercent) {
      failures.push(
        `overall coverage dropped by ${drop.toFixed(2)}% (allowed ${thresholds.maxOverallCoverageDropPercent.toFixed(2)}%)`,
      );
    }

    if (thresholds.failOnNewMissingCritical) {
      const baselineMap = new Map(
        baseline.fields.map((f) => [fieldKey(f.rootType, f.field), f.status]),
      );
      for (const rule of critical.criticalFields) {
        const key = fieldKey(rule.rootType, rule.field);
        const now = report.fields.find((f) => f.rootType === rule.rootType && f.field === rule.field);
        if (!now) continue;
        const before = baselineMap.get(key);
        if (before && before !== 'missing' && now.status === 'missing') {
          failures.push(`critical field regressed to missing: ${key}`);
        }
      }
    }
  }

  if (thresholds.failOnCriticalRequirements) {
    for (const rule of critical.criticalFields) {
      const match = report.fields.find((f) => f.rootType === rule.rootType && f.field === rule.field);
      const key = fieldKey(rule.rootType, rule.field);
      if (!match) {
        failures.push(`critical field missing from schema report: ${key}`);
        continue;
      }
      for (const facet of rule.requiredFacets) {
        if (!match.facets.includes(facet)) failures.push(`critical field ${key} missing required facet: ${facet}`);
      }
      if (match.projects.length < rule.minimumProjects) {
        failures.push(
          `critical field ${key} has ${match.projects.length} project(s), requires ${rule.minimumProjects}`,
        );
      }
    }
  }

  if (failures.length > 0) {
    throw new Error(`Coverage checks failed:\n- ${failures.join('\n- ')}`);
  }
};
