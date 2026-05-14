import type { Reporter, Task, UserConsoleLog } from 'vitest';
import logUpdate from 'log-update';

export default class CustomReporter implements Reporter {
  private passed = 0;
  private failed = 0;
  private skipped = 0;

  // By defining this but not printing, we suppress all test console logs
  // which prevents the "playwright stuff" from breaking the single-line output.
  onUserConsoleLog(log: UserConsoleLog) {
     return false; // Intentionally return false to suppress logs
  }

  onTestFinished(test: Task) {
    if (test.type !== 'test') return;
    
    if (test.result?.state === 'pass') {
        this.passed++;
    } else if (test.result?.state === 'fail') {
        this.failed++;
        logUpdate.clear();
        console.log(`\n❌ Failed: ${test.name}`);
        if (test.result.errors) {
            test.result.errors.forEach(err => {
                const errorMessage = err instanceof Error ? err.stack || err.message : String(err);
                console.log(errorMessage);
            });
        }
        console.log('');
    } else if (test.result?.state === 'skip' || test.result?.state === 'todo') {
        this.skipped++;
    }

    this.render();
  }

  render() {
     logUpdate(`✅ Passed: ${this.passed} | ⏭️ Skipped: ${this.skipped} | ❌ Failed: ${this.failed}`);
  }

  onFinished() {
     logUpdate.done();
     console.log(`\nTests Completed - Passed: ${this.passed} | Skipped: ${this.skipped} | Failed: ${this.failed}`);
  }
}
