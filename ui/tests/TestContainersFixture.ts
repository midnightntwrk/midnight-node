import { DockerComposeEnvironment, type StartedDockerComposeEnvironment, Wait, PullPolicy, ImagePullPolicy } from 'testcontainers';
import {fileURLToPath} from 'url'
import logging from './utils/Logger'
import { StartedGenericContainer } from 'testcontainers/build/generic-container/started-generic-container';

const __filename = fileURLToPath(import.meta.url)
const _logger = logging(__filename)


export async function useTestContainersFixture(dockerComposeLocation: string): Promise<TestContainersFixture> {
  let fixture: TestContainersFixture;

    _logger.info('Spinning up test environment...');
    const uid = '1';
    
    const composeEnvironment: StartedDockerComposeEnvironment = await new DockerComposeEnvironment('./', dockerComposeLocation)
      .withWaitStrategy(`node-${uid}`, Wait.forLogMessage("Running JSON-RPC server"))
      .withEnvironment({ TESTCONTAINERS_UID: uid })
      .up();
 
    _logger.info('Test environment started');
    fixture = new TestContainersFixture(composeEnvironment, uid);

  // eslint-disable-next-line @typescript-eslint/no-non-null-assertion
  return fixture;
}

export class TestContainersFixture {
  constructor(
    public readonly composeEnvironment: StartedDockerComposeEnvironment,
    private readonly uid: string,
  ) {}

  public async down(): Promise<void> {
    _logger.info('Tearing down test environment...');
    await this.composeEnvironment.down();
    _logger.info('Test environment torn down');
  }

  public static readonly NODE_PORT_WS = 9944;
  public static readonly NODE_HOST = "localhost";

  public getNodeWs(): string {
    const node: StartedGenericContainer = this.composeEnvironment.getContainer(`node-${this.uid}`);
    const nodePortWs = node.getMappedPort(TestContainersFixture.NODE_PORT_WS);
    return `ws://${TestContainersFixture.NODE_HOST}:${nodePortWs}`;
  }
}
