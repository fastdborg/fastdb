'use strict';
try {
  module.exports = require('./fastdb.node');
} catch (cause) {
  const error = new Error(
    `FastDB could not load its native addon for ${process.platform}/${process.arch} on Node ${process.versions.node}. ` +
    'Use an addon built for this platform; for a source checkout, run fastdb/scripts/check-node.sh from the repository root. ' +
    'See the package README for supported build and installation instructions.',
    { cause },
  );
  error.code = 'FDB_NATIVE_LOAD';
  throw error;
}
