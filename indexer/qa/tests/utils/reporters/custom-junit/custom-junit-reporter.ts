// This file is part of midnightntwrk/midnight-indexer
// Copyright (C) 2025 Midnight Foundation
// SPDX-License-Identifier: Apache-2.0
// Licensed under the Apache License, Version 2.0 (the "License");
// You may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

import fs from 'fs';
import path from 'path';
import { RunnerTestSuite, RunnerTaskResult } from 'vitest';
import { Reporter } from 'vitest/reporters';
import { create } from 'xmlbuilder2';

function flattenTests(suite: RunnerTestSuite, parentNames: string[] = []) {
  const results: {
    suiteName: string;
    className: string;
    testName: string;
    time: number;
    failureMessage?: string;
  }[] = [];

  const currentNames = [...parentNames, suite.name];

  for (const task of suite.tasks) {
    if (task.type === 'suite') {
      results.push(...flattenTests(task, currentNames));
    } else if (task.type === 'test') {
      const result = task.result as RunnerTaskResult;

      // You can use the following information but note that not everything
      // might be available, especially the "describe()" strings
      // currentNames[0] -> This is the test file path
      // currentNames[1] -> This is the top level describe string
      // suite.name      -> This is the lower level describe string, just wrapping the test/it
      // task.name       -> This is the test/it string
      results.push({
        suiteName: currentNames[0] || suite.name,
        className: currentNames[1] || '',
        testName: `${suite.name || ''} ${task.name}`,
        time: result?.duration ? result.duration / 1000 : 0,
        failureMessage: result?.errors?.[0]?.message,
      });
    }
  }

  return results;
}

export default class CustomJUnitReporter implements Reporter {
  onFinished(files: RunnerTestSuite[]) {
    const testcases = files.flatMap((file) => flattenTests(file));

    const grouped = testcases.reduce(
      (acc, tc) => {
        if (!acc[tc.suiteName]) acc[tc.suiteName] = [];
        acc[tc.suiteName].push(tc);
        return acc;
      },
      {} as Record<string, typeof testcases>,
    );

    const xml = create({ version: '1.0' }).ele('testsuites');

    for (const [suiteName, cases] of Object.entries(grouped)) {
      const suite = xml.ele('testsuite', {
        name: suiteName,
        tests: cases.length,
        failures: cases.filter((t) => !!t.failureMessage).length,
        errors: 0,
        time: cases.reduce((sum, c) => sum + c.time, 0).toFixed(3),
      });

      for (const test of cases) {
        const testcase = suite.ele('testcase', {
          classname: test.className,
          name: test.testName,
          time: test.time.toFixed(3),
        });

        if (test.failureMessage) {
          testcase.ele('failure').txt(test.failureMessage);
        }
      }
    }

    const xmlString = xml.end({ prettyPrint: true });
    const outputPath = path.resolve('./reports/custom-junit-report.xml');
    fs.mkdirSync(path.dirname(outputPath), { recursive: true });
    fs.writeFileSync(outputPath, xmlString, 'utf-8');
    console.log(`Custom JUnit report written to ${outputPath}`);
  }
}
