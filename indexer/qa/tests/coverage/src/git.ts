import { execSync } from 'child_process';

const safeExec = (command: string, cwd: string): string => {
  try {
    return execSync(command, { cwd, stdio: ['ignore', 'pipe', 'ignore'] }).toString().trim();
  } catch {
    return 'unknown';
  }
};

export const getBranchName = (repoRoot: string): string =>
  safeExec('git rev-parse --abbrev-ref HEAD', repoRoot);

export const getCommitSha = (repoRoot: string): string => safeExec('git rev-parse HEAD', repoRoot);
