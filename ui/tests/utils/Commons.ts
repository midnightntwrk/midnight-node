/* eslint-disable @typescript-eslint/no-explicit-any */
import fs from 'fs'
import {fileURLToPath} from 'url'
import logging from './Logger'
const __filename = fileURLToPath(import.meta.url)
const _logger = logging(__filename)

export class Commons {
  public static sleep(ms: number): Promise<void> {
    return new Promise((resolve) => setTimeout(resolve, ms))
  }

  public static getJsonFromFile(file: string): any {
    return JSON.stringify(JSON.parse(Commons.getTxTemplate(file)))
  }

  public static getTxTemplate(file: string, directory = 'test-contract'): any {
    const filePath = `../../res/${directory}/${file}`
    _logger.info(`Reading file=${filePath}`)
    return fs.readFileSync(filePath).toString('hex')
  }
}
