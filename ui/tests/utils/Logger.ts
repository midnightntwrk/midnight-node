import path from 'path'
import pkg from 'winston'
const {createLogger, format, transports} = pkg
import DailyRotateFile from 'winston-daily-rotate-file'

const {combine, timestamp, printf} = format

const baseFormat = combine(
  timestamp({format: 'YYYY-MM-DD HH:mm:ss,SSS'}),
  printf(({level, message, timestamp, ...metadata}) => {
    return `${timestamp} ${level.toUpperCase()} [${metadata.class}] - ${message}`
  }),
)

const options = {
  level: 'debug',
  transports: [
    new transports.Console({
      level: 'info',
    }),
    new DailyRotateFile({
      level: 'debug',
      filename: 'logs/midnight-e2e-automation.%DATE%.log',
      datePattern: 'YYYY-MM-DD',
      maxSize: '100m',
      maxFiles: '2d',
      zippedArchive: true,
    }),
  ],
}

export default (moduleName: string): pkg.Logger =>
  createLogger({...options, format: baseFormat, defaultMeta: {class: path.basename(moduleName)}})
