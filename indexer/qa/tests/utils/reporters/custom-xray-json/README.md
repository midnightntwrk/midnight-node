# Custom XRay JSON reporter

This reporter is customised to be used in conjunction with XRay. It will produce a JSON report with a format that is compatible with XRay JSON results (explained in details here https://docs.getxray.app/space/XRAYCLOUD/44577176/Import+Execution+Results+-+REST+v2#Xray-JSON-Results)

# Usage

## Add to the reporters

From an usage point of view, you will need to enable it in Vitest configuration file. Just import it

``` typescript
import XRayJsonReporter from './utils/reporters/custom-xray-json/xray-json-reporter';
```

and add it to the list of reporters, as simple as adding one line on the below section

``` Typescript
reporters: [
      'verbose',
      new XRayJsonReporter(), // Add this entry
      new CustomJUnitReporter(),
      [
        'junit',
        {
          outputFile: './reports/junit/test-results.xml',
        },
      ],
      [
        'json',
        {
          outputFile: './reports/json/test-results.json',
        },
      ],
    ],
```

## In your tests

By default, the XRay reporter will pick up the test case result and some basic information for the test results. That can be populated with additional information, such as labels/tags like in the example below.

NOTE 1: labels and key (Jira ID of a XRay test case) are the only fields supported. They are both optional.

NOTE 2: If you don't provide a Jira Key it likely means you don't have already a XRay test case, in which case XRay will create one for you and match it by the test name (in the example below 'this is a test', prefixed by a test suite name if defined). So try not to change the test name, or if you have to, add a key to keep the mapping consistent

``` Typescript
test('this is a test', async ({task}) => {

      // Use the task metadata like in the below example
      // to export custom fields
      // So far labels and Jira key are the only supporterd fields
      // ... this can be extended
      task.meta.custom = {
        labels: ['UnshieldedTokens', 'Subscription', 'Transaction'],
        testKey: 'PM-1234'
      };

      // ... rest of your test here
});

```

## Useful variables
The reporter uses environment variables to define some parts of the report that will be interpreted accordingly by XRay.

- __TARGET_ENV__: this is a mandatory field needed to understand what environment the test execution is targetting. If not specified 'undeployed' will be used, which might be what you want, so setting up explicitly every time is best

- __XRAY_COMPONENT__: this is a mandatory field needed to define what component the test execution is for

- __XRAY_TEST_EXEC_KEY__: This is the test execution key (a Jira ID for the TestExecution type). At least one of XRAY_TEST_EXEC_KEY or XRAY_TEST_PLAN_KEY must be provided.

- __XRAY_TEST_PLAN_KEY__: This is the test plan key (a Jira ID for the TestPlan type). At least one of XRAY_TEST_EXEC_KEY or XRAY_TEST_PLAN_KEY must be provided.

- __XRAY_PROJECT_KEY__: This is the optional project Key. If not provided the default is 'PM' for Porject Midnight

- __XRAY_REPORT_TESTS_MISSING_METADATA__: This controls whether tests without custom metadata should be included in the XRay report. When set to 'true', tests that don't define custom metadata will be excluded from the report. When set to 'false' (default) or not set, all tests will be included regardless of metadata. This is useful when you only want to report specific tests that have been properly configured with XRay metadata.

_NOTE: Please don't confuse test key, test plan key and test execution key as they are identifying different entities_

# Upload the results

Once you have executed your tests (either locally or in CI), a JSON test result file should be produced in `reports/xray/test-results.json`. You can use that to upload the test execution report either from your local environment through a script or setup GitHub workflow to do it for you. In both cases you will need the following environment variables set

- __XRAY_CLIENT_ID__ and __XRAY_CLIENT_SECRET__

## Locally through a script

You can invoke the `upload-to-xray.sh` script, bearing in mind it will expect to have the report file in `reports/xray/test-results.json`

## Through CI execution in GitHub

For now the GitHub workflow will need to use the same script as above, in future we will create a GitHub Action

# Be aware of ...

## Providing a test key
As we already said, if you don't provide a test key, XRay will try to match an existing test case using the `definition` field. If it finds one it will use that test case, otherwise it will create one.

## Fields to update

- __summary__: This is the title of the XRay test case that appears on top of the Jira issue. This is created using [test suite name], [test case name] format (comma-separated)

- __labels__: When providing the labels remember that they will be added to the existing ones. If labels already exist, the ones you are providing will be added. Because of this behaviour, the labels you provide might not be the ones that you actually see, just because of what we said.

- __definition__: The definition field of a test case will be set based the [test suite name].[test case name]. So basically test suite and test case names joined together by a dot