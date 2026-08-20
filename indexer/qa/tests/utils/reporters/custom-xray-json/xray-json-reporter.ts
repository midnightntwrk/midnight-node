// This file is part of midnightntwrk/midnight-indexer.
// Copyright (C) Midnight Foundation
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
import { Reporter } from 'vitest/reporters';
import { RunnerTestSuite, RunnerTaskResult } from 'vitest';

// XRay JSON format interfaces based on the schema
// TODOs:
// - Add support for additional metadata to be added to the test results (with explicit types)
// - Add support test retries so that only the latest test result is used, not all
// - Check the exact expectations for the testKey presence
// - Check the exact expectations for the testPlanKey and/or testExecutionKey presence

interface XRayExecInfo {
  summary: string;
  description?: string;
  testPlanKey?: string;
  testEnvironments?: string[];
  startDate: string;
  finishDate: string;
  testExecutionKey?: string;
}

interface XRayTest {
  testKey?: string;
  start: string;
  finish: string;
  status: 'PASSED' | 'FAILED' | 'TODO' | 'EXECUTING' | 'ABORTED' | 'SKIPPED';
  assignee?: string;
  testInfo?: XRayTestInfo;
  evidences?: Array<{
    data: string;
    filename: string;
    contentType: string;
  }>;
  comment?: string;
  defects?: string[];
  examples?: Array<{
    status: 'PASSED' | 'FAILED' | 'TODO' | 'EXECUTING' | 'ABORTED' | 'SKIPPED';
    duration?: number;
    defects?: string[];
    comment?: string;
  }>;
}

interface XRayTestInfo {
  projectKey: string;
  type: string;
  summary: string;
  definition: string;
  labels?: string[];
}

interface XRayReport {
  testExecutionKey?: string;
  info: XRayExecInfo;
  tests: XRayTest[];
}

function flattenTests(
  suite: RunnerTestSuite,
  parentNames: string[] = [],
): {
  suiteName: string;
  className: string;
  testName: string;
  time: number;
  failureMessage?: string;
  startTime?: number;
  metadata?: Record<string, unknown>;
}[] {
  const results: {
    suiteName: string;
    className: string;
    testName: string;
    time: number;
    failureMessage?: string;
    startTime?: number;
    metadata?: Record<string, unknown>;
  }[] = [];

  const currentNames = [...parentNames, suite.name];

  for (const task of suite.tasks) {
    if (task.type === 'suite') {
      results.push(...flattenTests(task, currentNames));
    } else if (task.type === 'test') {
      const result = task.result as RunnerTaskResult;

      // Extract timing information
      const startTime = result?.startTime || Date.now();
      const duration = result?.duration || 0;

      // You can use the following information but note that not everything
      // might be available, especially the "describe()" strings
      // currentNames[0] -> This is the test file path
      // currentNames[1] -> This is the top level describe string
      // suite.name      -> This is the lower level describe string, just wrapping the test/it
      // task.name       -> This is the test/it string

      const testFileName = currentNames[0];
      const testSuiteName = suite.name;
      const testTaskName = task.name;

      // console.debug('suiteName: ', suite.name);
      // console.debug('currentNames[0]: ', currentNames[0]);
      // console.debug('currentNames[1]: ', currentNames[1]);
      // console.debug('suite.name: ', suite.name);
      // console.debug('task.name: ', task.name);
      // console.debug('--------------------------------');
      results.push({
        suiteName: testFileName,
        className: testSuiteName,
        testName: testTaskName,
        time: duration / 1000, // Convert to seconds
        failureMessage: result?.errors?.[0]?.message,
        startTime: startTime,
        metadata: (task as unknown as { meta: Record<string, unknown> }).meta, // Extract task metadata
      });

      // console.debug('results.at(-1).suiteName: ', results.at(-1)?.suiteName);
      // console.debug('results.at(-1).className: ', results.at(-1)?.className);
      // console.debug('results.at(-1).testName: ', results.at(-1)?.testName);
    }
  }

  return results;
}

export default class XRayJsonReporter implements Reporter {
  private startTime: number = Date.now();

  onFinished(files: RunnerTestSuite[]) {
    const testcases = files.flatMap((file) => flattenTests(file));
    const endTime = Date.now();

    if (!process.env.XRAY_COMPONENT) {
      console.warn(
        'WARNING: XRay JSON Reporter Error: XRAY_COMPONENT environment variable must be defined',
      );
      console.warn(
        '       Please set the XRAY_COMPONENT environment variable to specify the component being tested',
      );
      console.warn('WARNING: Failed to create a Custom XRay JSON report');
      return;
    }

    if (!process.env.XRAY_PROJECT_KEY) {
      console.warn('WARNING: XRAY_PROJECT_KEY env variable not set, PM will be used as default');
    }

    const targetEnv = process.env.TARGET_ENV || 'undeployed';
    const xrayComponent = process.env.XRAY_COMPONENT;
    const xrayReportEmptyMeta = process.env.XRAY_REPORT_TESTS_MISSING_METADATA || 'false';
    const xrayTestExecKey = process.env.XRAY_TEST_EXEC_KEY;
    const xrayTestPlanKey = process.env.XRAY_TEST_PLAN_KEY;
    const xrayProjectKey = process.env.XRAY_PROJECT_KEY || 'PM';

    console.debug('XRay JSON Reporter info: ');
    console.debug(` TARGET_ENV: ${targetEnv}`);
    console.debug(` XRAY_COMPONENT: ${xrayComponent}`);
    console.debug(` XRAY_REPORT_TESTS_MISSING_METADATA: ${xrayReportEmptyMeta}`);
    console.debug(` XRAY_TEST_EXEC_KEY: ${xrayTestExecKey}`);
    console.debug(` XRAY_TEST_PLAN_KEY: ${xrayTestPlanKey}`);
    console.debug(` XRAY_PROJECT_KEY: ${xrayProjectKey}`);

    // Group tests by suite for better organization
    // const grouped = testcases.reduce(
    //   (acc, tc) => {
    //     if (!acc[tc.suiteName]) acc[tc.suiteName] = [];
    //     acc[tc.suiteName].push(tc);
    //     return acc;
    //   },
    //   {} as Record<string, typeof testcases>,
    // );

    // Convert test cases to XRay format
    const xrayTests: XRayTest[] = testcases.flatMap((test) => {
      const status: XRayTest['status'] = test.failureMessage ? 'FAILED' : 'PASSED';

      const testData: XRayTest = {
        start: new Date(test.startTime || this.startTime).toISOString(),
        finish: new Date((test.startTime || this.startTime) + test.time * 1000).toISOString(),
        status,
      };

      // Skip the test entirely if the metadata is not defined
      if (xrayReportEmptyMeta === 'false' && test.metadata?.custom === undefined) {
        return [];
      }

      // Details about the schema here https://docs.getxray.app/space/XRAYCLOUD/44565311/Using+Xray+JSON+format+to+import+execution+results
      // Minimum required fields when testInfo is provided: ["summary", "projectKey", "type"],
      testData.testInfo = {
        projectKey: xrayProjectKey,
        type: 'Automated',
        summary: `${test.className}, ${test.testName}`,
        definition: `${test.className}.${test.testName}`,
      };

      // Managing labels as metadata
      testData.testInfo.labels = ['Xray']; // Default label
      if (test.metadata?.custom) {
        const custom = test.metadata.custom as { labels?: string[] };
        if (custom.labels) {
          testData.testInfo.labels.push(...custom.labels);
        }
      }

      // Managing the test key if available
      if (test.testName && test.metadata?.custom) {
        const custom = test.metadata.custom as { testKey?: string };
        if (custom.testKey) {
          testData.testKey = custom.testKey;
        }
      }

      // Add failure evidence if test failed
      if (test.failureMessage) {
        testData.evidences = [
          {
            data: test.failureMessage,
            contentType: 'text/plain',
            filename: test.suiteName,
          },
        ];
        testData.comment = `Test failed: ${test.failureMessage}`;
      }

      return testData;
    });

    const now = new Date();
    const currentDate = now.toLocaleDateString('en-GB'); // DD/MM/YYYY format
    const currentTime = now.toLocaleTimeString('en-GB', {
      timeZoneName: 'short',
    }); // HH:MM:SS with timezone

    // Create XRay report
    const xrayReport: XRayReport = {
      info: {
        summary: `Test execution for ${xrayComponent} tests targeting ${targetEnv} env ran on ${currentDate} at ${currentTime}`,
        description:
          `Automated test execution results for ${xrayComponent} tests. ` +
          `This XRay issue has been automatically created by XRay`,
        startDate: new Date(this.startTime).toISOString(),
        finishDate: new Date(endTime).toISOString(),
        testEnvironments: [targetEnv],
      },
      tests: xrayTests,
    };

    // Validate that at least one of the required XRay keys is provided
    if (!xrayTestExecKey && !xrayTestPlanKey) {
      console.error(
        'ERROR: XRay JSON Reporter Error: At least one of XRAY_TEST_EXEC_KEY or XRAY_TEST_PLAN_KEY must be defined',
      );
      console.error('       Please set one of these environment variables:');
      console.error('        - XRAY_TEST_EXEC_KEY: For linking to an existing test execution');
      console.error('        - XRAY_TEST_PLAN_KEY: For linking to a test plan');
      console.error('ERROR: Failed to create a Custom XRay JSON report');
      return;
      //throw new Error('Missing required XRay configuration: XRAY_TEST_EXEC_KEY or XRAY_TEST_PLAN_KEY must be defined');
    }

    if (xrayTestExecKey) {
      xrayReport.testExecutionKey = xrayTestExecKey;
    }

    if (xrayTestPlanKey) {
      xrayReport.info.testPlanKey = xrayTestPlanKey;
    }

    // Write XRay JSON report
    const outputPath = path.resolve('./reports/xray/test-results.json');
    fs.mkdirSync(path.dirname(outputPath), { recursive: true });
    fs.writeFileSync(outputPath, JSON.stringify(xrayReport, null, 2), 'utf-8');

    console.log(`Custom XRay JSON report written to ${outputPath}`);
  }
}
