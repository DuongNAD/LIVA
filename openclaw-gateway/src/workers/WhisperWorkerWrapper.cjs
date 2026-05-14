const { workerData } = require('node:worker_threads');
require('tsx/cjs');
require(workerData.tsPath);
