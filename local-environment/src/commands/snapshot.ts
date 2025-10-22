// This file is part of midnight-node.
// Copyright (C) 2025 Midnight Foundation
// SPDX-License-Identifier: Apache-2.0
// Licensed under the Apache License, Version 2.0 (the "License");
// You may not use this file except in compliance with the License.
// You may obtain a copy of the License at
// http://www.apache.org/licenses/LICENSE-2.0
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

import { execSync, ExecSyncOptions } from "child_process";
import { existsSync, readFileSync } from "fs";
import path from "path";
import YAML from "yaml";
import { SnapshotOptions } from "../lib/types";
import {
  ensureSnapshotCredentials,
  SnapshotCredentials,
} from "../lib/snapshotEnv";

const DEFAULT_SNAPSHOT_IMAGE =
  process.env.MN_SNAPSHOT_IMAGE ?? "amazon/aws-cli:2.17.16";
const DEFAULT_S3_URI = process.env.MN_SNAPSHOT_S3_URI ?? "";
const DEFAULT_TIMEOUT_MINUTES = 30;

interface KubernetesMetadata {
  name?: string;
}

interface KubernetesStatefulSetSpec {
  replicas?: number;
}

interface KubernetesStatefulSet {
  spec?: KubernetesStatefulSetSpec;
}

interface KubernetesPersistentVolumeClaim {
  metadata?: KubernetesMetadata;
}

interface KubernetesPersistentVolumeClaimList {
  items?: KubernetesPersistentVolumeClaim[];
}

const SNAPSHOT_SCRIPT = (() => {
  const snapshot_script_path = path.resolve(
    __dirname,
    "..",
    "scripts",
    "snapshot.sh",
  );
  if (existsSync(snapshot_script_path)) {
    return readFileSync(snapshot_script_path, "utf-8");
  }

  throw new Error("Snapshot script could not be located");
})();

const sleep = (ms: number) =>
  new Promise<void>((resolve) => setTimeout(resolve, ms));

export async function snapshot(
  namespace: string,
  options: SnapshotOptions,
): Promise<void> {
  // TODO: choose node
  const bootnodeStatefulSet =
    options.bootnodeStatefulSet ?? "midnight-node-boot-01";
  const snapshotPodName = `${bootnodeStatefulSet}-snapshot`;
  const snapshotImage = options.snapshotImage ?? DEFAULT_SNAPSHOT_IMAGE;
  const s3Uri = options.s3Uri ?? DEFAULT_S3_URI;
  const timeoutMinutes = options.timeoutMinutes ?? DEFAULT_TIMEOUT_MINUTES;
  const timeoutSeconds = Math.max(Math.floor(timeoutMinutes * 60), 30);

  const snapshotEnv = cleanProcessEnv();
  const snapshotCredentials = ensureSnapshotCredentials(snapshotEnv);
  const credentialsSecretName = `${snapshotPodName}-aws-credentials`;

  if (!s3Uri) {
    throw new Error(
      "No S3 URI provided. Pass --s3-uri or set MN_SNAPSHOT_S3_URI.",
    );
  }

  console.log(
    `Creating snapshot for ${bootnodeStatefulSet} in namespace ${namespace}`,
  );

  const statefulSetInfo = getStatefulSet(namespace, bootnodeStatefulSet);
  const originalReplicas = statefulSetInfo?.spec?.replicas ?? 1;

  let snapshotPodCreated = false;
  let bootnodeScaledDown = false;
  let snapshotSecretCreated = false;

  try {
    const secretManifest = buildSnapshotSecretManifest({
      namespace,
      secretName: credentialsSecretName,
      credentials: snapshotCredentials,
    });
    applyManifest(secretManifest);
    snapshotSecretCreated = true;

    console.log(`Scaling ${bootnodeStatefulSet} down to 0 replicas`);
    scaleStatefulSet(namespace, bootnodeStatefulSet, 0);
    bootnodeScaledDown = true;

    await waitForPodDeletion(
      namespace,
      `${bootnodeStatefulSet}-0`,
      timeoutSeconds,
    );

    const pvcName =
      options.pvcName ?? resolveBootnodePvc(namespace, bootnodeStatefulSet);
    if (!pvcName) {
      throw new Error(
        `Unable to determine PVC name for ${bootnodeStatefulSet}. ` +
          "Provide --pvc to override.",
      );
    }

    console.log(
      `Creating snapshot pod ${snapshotPodName} mounting PVC ${pvcName}`,
    );

    deleteSnapshotPod(namespace, snapshotPodName);

    const manifest = buildSnapshotPodManifest({
      namespace,
      podName: snapshotPodName,
      pvcName,
      snapshotImage,
      s3Uri,
      bootnodeStatefulSet,
      credentialsSecretName,
      endpointUrl: snapshotCredentials.endpointUrl,
      includeSessionToken: Boolean(snapshotCredentials.sessionToken),
    });

    applyManifest(manifest);
    snapshotPodCreated = true;

    await waitForSnapshotCompletion(namespace, snapshotPodName, timeoutSeconds);

    console.log(`Snapshot pod ${snapshotPodName} completed successfully`);
  } catch (error) {
    console.error(`Snapshot failed: ${(error as Error).message}`);
    throw error;
  } finally {
    if (snapshotPodCreated) {
      deleteSnapshotPod(namespace, snapshotPodName);
    }

    if (snapshotSecretCreated) {
      deleteSecret(namespace, credentialsSecretName);
    }

    if (bootnodeScaledDown) {
      console.log(
        `Restoring ${bootnodeStatefulSet} replicas to ${originalReplicas}`,
      );
      scaleStatefulSet(namespace, bootnodeStatefulSet, originalReplicas);
      waitForStatefulSetReady(namespace, bootnodeStatefulSet, timeoutSeconds);
    }
  }
}

function getStatefulSet(
  namespace: string,
  name: string,
): KubernetesStatefulSet {
  try {
    const stdout = execSync(
      `kubectl get statefulset ${name} -n ${namespace} -o json`,
      execOptions(),
    ).toString();
    return JSON.parse(stdout) as KubernetesStatefulSet;
  } catch (error) {
    throw new Error(
      `Failed to read statefulset ${name} in namespace ${namespace}: ${(error as Error).message}`,
    );
  }
}

function scaleStatefulSet(
  namespace: string,
  name: string,
  replicas: number,
): void {
  execSync(
    `kubectl scale statefulset ${name} --replicas=${replicas} -n ${namespace}`,
    execOptions({ stdio: ["ignore", "inherit", "inherit"] }),
  );
}

async function waitForPodDeletion(
  namespace: string,
  podName: string,
  timeoutSeconds: number,
): Promise<void> {
  if (!podExists(namespace, podName)) {
    return;
  }

  console.log(`Waiting for pod ${podName} to terminate`);
  execSync(
    `kubectl wait --for=delete pod/${podName} -n ${namespace} --timeout=${timeoutSeconds}s`,
    execOptions({ stdio: ["ignore", "inherit", "inherit"] }),
  );
}

function podExists(namespace: string, podName: string): boolean {
  try {
    execSync(
      `kubectl get pod ${podName} -n ${namespace}`,
      execOptions({ stdio: "ignore" }),
    );
    return true;
  } catch (error) {
    const err = error as Error & { stderr?: Buffer };
    const stderr = err.stderr?.toString() ?? "";
    if (stderr.includes("NotFound")) {
      return false;
    }
    throw error;
  }
}

function resolveBootnodePvc(namespace: string, bootnode: string): string {
  try {
    const stdout = execSync(
      `kubectl get pvc -n ${namespace} -o json`,
      execOptions(),
    ).toString();
    const parsed = JSON.parse(stdout) as KubernetesPersistentVolumeClaimList;
    const pvc = parsed.items?.find((item) =>
      item?.metadata?.name?.includes(`${bootnode}`),
    );
    if (!pvc) {
      throw new Error("no matching PVC found");
    }
    const pvcName = pvc.metadata?.name;
    if (!pvcName) {
      throw new Error("matched PVC is missing a name");
    }
    return pvcName;
  } catch (error) {
    throw new Error(
      `Failed to resolve PVC for ${bootnode}: ${(error as Error).message}`,
    );
  }
}

function buildSnapshotSecretManifest(params: {
  namespace: string;
  secretName: string;
  credentials: SnapshotCredentials;
}): Record<string, unknown> {
  const { namespace, secretName, credentials } = params;
  const stringData: Record<string, string> = {
    AWS_ACCESS_KEY_ID: credentials.accessKeyId,
    AWS_SECRET_ACCESS_KEY: credentials.secretAccessKey,
  };

  if (credentials.sessionToken) {
    stringData.AWS_SESSION_TOKEN = credentials.sessionToken;
  }

  return {
    apiVersion: "v1",
    kind: "Secret",
    metadata: {
      name: secretName,
      namespace,
    },
    type: "Opaque",
    stringData,
  };
}

function buildSnapshotPodManifest(params: {
  namespace: string;
  podName: string;
  pvcName: string;
  snapshotImage: string;
  s3Uri: string;
  bootnodeStatefulSet: string;
  credentialsSecretName: string;
  endpointUrl: string;
  includeSessionToken: boolean;
}): Record<string, unknown> {
  const {
    namespace,
    podName,
    pvcName,
    snapshotImage,
    s3Uri,
    bootnodeStatefulSet,
    credentialsSecretName,
    endpointUrl,
    includeSessionToken,
  } = params;

  return {
    apiVersion: "v1",
    kind: "Pod",
    metadata: {
      name: podName,
      namespace,
      labels: {
        "app.kubernetes.io/name": "midnight-node-snapshotper",
        "midnight.tech/bootnode": bootnodeStatefulSet,
      },
    },
    spec: {
      restartPolicy: "Never",
      containers: [
        {
          name: "snapshot",
          image: snapshotImage,
          imagePullPolicy: "IfNotPresent",
          command: ["/bin/sh", "-c"],
          args: [SNAPSHOT_SCRIPT],
          env: [
            { name: "SNAPSHOT_S3_URI", value: s3Uri },
            { name: "SNAPSHOT_S3_ENDPOINT_URL", value: endpointUrl },
            { name: "BOOTNODE_NAME", value: bootnodeStatefulSet },
            {
              name: "AWS_ACCESS_KEY_ID",
              valueFrom: {
                secretKeyRef: {
                  name: credentialsSecretName,
                  key: "AWS_ACCESS_KEY_ID",
                },
              },
            },
            {
              name: "AWS_SECRET_ACCESS_KEY",
              valueFrom: {
                secretKeyRef: {
                  name: credentialsSecretName,
                  key: "AWS_SECRET_ACCESS_KEY",
                },
              },
            },
            ...(includeSessionToken
              ? [
                  {
                    name: "AWS_SESSION_TOKEN",
                    valueFrom: {
                      secretKeyRef: {
                        name: credentialsSecretName,
                        key: "AWS_SESSION_TOKEN",
                      },
                    },
                  },
                ]
              : []),
          ],
          volumeMounts: [
            {
              name: "node-data",
              mountPath: "/node",
            },
          ],
        },
      ],
      volumes: [
        {
          name: "node-data",
          persistentVolumeClaim: { claimName: pvcName },
        },
      ],
    },
  };
}

function applyManifest(manifest: Record<string, unknown>): void {
  const yaml = YAML.stringify(manifest);
  execSync(
    `kubectl apply -f -`,
    execOptions({ input: yaml, stdio: ["pipe", "inherit", "inherit"] }),
  );
}

async function waitForSnapshotCompletion(
  namespace: string,
  podName: string,
  timeoutSeconds: number,
): Promise<void> {
  const timeoutMs = timeoutSeconds * 1000;
  const start = Date.now();
  while (Date.now() - start < timeoutMs) {
    try {
      const stdout = execSync(
        `kubectl get pod ${podName} -n ${namespace} -o json`,
        execOptions(),
      ).toString();
      const pod = JSON.parse(stdout);
      const phase = pod?.status?.phase as string | undefined;
      if (phase === "Succeeded") {
        return;
      }
      if (phase === "Failed") {
        const logs = getPodLogs(namespace, podName);
        throw new Error(`Snapshot pod failed. Logs:\n${logs}`);
      }
    } catch (error) {
      const err = error as Error & { stderr?: Buffer };
      const stderr = err.stderr?.toString() ?? "";
      if (stderr.includes("NotFound")) {
        // Pod not yet created, keep waiting
      } else if (stderr.trim()) {
        console.warn(stderr.trim());
      }
    }
    await sleep(5000);
  }
  const logs = getPodLogs(namespace, podName);
  throw new Error(
    `Timed out waiting for snapshot pod ${podName}. Latest logs:\n${logs}`,
  );
}

function getPodLogs(namespace: string, podName: string): string {
  try {
    return execSync(
      `kubectl logs ${podName} -n ${namespace}`,
      execOptions(),
    ).toString();
  } catch (error) {
    return (error as Error & { stderr?: Buffer }).stderr?.toString() ?? "";
  }
}

function deleteSnapshotPod(namespace: string, podName: string): void {
  execSync(
    `kubectl delete pod ${podName} -n ${namespace} --ignore-not-found`,
    execOptions({ stdio: ["ignore", "inherit", "inherit"] }),
  );
}

function deleteSecret(namespace: string, name: string): void {
  execSync(
    `kubectl delete secret ${name} -n ${namespace} --ignore-not-found`,
    execOptions({ stdio: ["ignore", "inherit", "inherit"] }),
  );
}

function cleanProcessEnv(): Record<string, string> {
  return Object.fromEntries(
    Object.entries(process.env).filter(
      ([, value]) => typeof value === "string",
    ),
  ) as Record<string, string>;
}

function waitForStatefulSetReady(
  namespace: string,
  name: string,
  timeoutSeconds: number,
): void {
  try {
    execSync(
      `kubectl rollout status statefulset/${name} -n ${namespace} --timeout=${timeoutSeconds}s`,
      execOptions({ stdio: ["ignore", "inherit", "inherit"] }),
    );
  } catch (error) {
    console.warn(
      `Timed out waiting for statefulset ${name} to be ready: ${(error as Error).message}`,
    );
  }
}

function execOptions(overrides: ExecSyncOptions = {}): ExecSyncOptions {
  return { encoding: "utf-8", ...overrides };
}