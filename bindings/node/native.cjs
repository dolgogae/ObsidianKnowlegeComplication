'use strict'

const path = require('node:path')

function targetName() {
  if (process.platform === 'darwin' && process.arch === 'arm64') return 'darwin-arm64'
  if (process.platform === 'darwin' && process.arch === 'x64') return 'darwin-x64'
  if (process.platform === 'linux' && process.arch === 'x64') {
    const report = process.report && process.report.getReport()
    if (!report || !report.header || !report.header.glibcVersionRuntime) {
      throw new Error('okc-compiler supports glibc Linux only; musl is not a release target')
    }
    return 'linux-x64-gnu'
  }
  if (process.platform === 'win32' && process.arch === 'x64') return 'win32-x64-msvc'
  throw new Error(`okc-compiler has no native build for ${process.platform}-${process.arch}`)
}

const target = targetName()
const localCandidates = [
  `okc-compiler.${target}.node`,
  'okc-compiler.node',
]

let lastError
for (const candidate of localCandidates) {
  try {
    module.exports = require(path.join(__dirname, candidate))
    return
  } catch (error) {
    if (error && error.code !== 'MODULE_NOT_FOUND') throw error
    lastError = error
  }
}

try {
  module.exports = require(`okc-compiler-${target}`)
} catch (error) {
  const reason = error && error.message ? error.message : String(error || lastError)
  throw new Error(`Could not load the okc-compiler native addon for ${target}: ${reason}`)
}
