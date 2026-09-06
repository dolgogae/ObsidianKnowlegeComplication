import assert from 'node:assert/strict'
import test from 'node:test'

import okc, { INTEROP_SCHEMA_VERSION, OkcClient } from '../index.js'

test('ESM and default exports share the public classes', () => {
  assert.equal(INTEROP_SCHEMA_VERSION, 2)
  assert.equal(okc.OkcClient, OkcClient)
  assert.equal(new OkcClient().apiInfo().apiVersion, '1')
})
