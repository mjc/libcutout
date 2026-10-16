import test from 'node:test';
import assert from 'node:assert/strict';
import { canReplaySignupAttempt, fetchWithTimeout, isCurrentWidgetGeneration, removalAttempt, removalPayload, signupAttempt, signupPayload } from './signup-contract.js';

const values = {
  email: ' rider@example.test ',
  pev_type: ' ',
  pev_model: '  Example Wheel  ',
  requested_accessories: '  RGB controller  '
};

test('signup payload normalization matches the server idempotency contract', () => {
  const payload = JSON.parse(signupPayload(values, 'en'));
  assert.deepEqual(payload, {
    email: 'rider@example.test',
    pev_type: null,
    pev_model: 'Example Wheel',
    requested_accessories: 'RGB controller',
    locale: 'en'
  });
  assert.equal(JSON.parse(signupPayload({ ...values, email: 'RIDER@example.test' }, 'en')).email, 'RIDER@example.test');
});

test('an ambiguous retry reuses the same body only for the same normalized payload', () => {
  const attempt = signupAttempt({
    values,
    locale: 'en',
    idempotencyKey: 'request-key',
    turnstileToken: 'spent-or-uncertain-token',
    website: ''
  });
  const unchanged = { ...values, email: 'rider@example.test', pev_model: 'Example Wheel', requested_accessories: 'RGB controller' };

  assert.equal(canReplaySignupAttempt(attempt, signupPayload(unchanged, 'en')), true);
  assert.equal(attempt.body.turnstile_token, 'spent-or-uncertain-token');
  assert.equal(attempt.body.idempotency_key, 'request-key');
  const changedValues = { ...values, pev_model: 'Changed after submit' };
  assert.equal(JSON.parse(signupPayload(changedValues, 'en')).pev_model, 'Changed after submit');
  assert.equal(attempt.body.pev_model, 'Example Wheel');
  assert.equal(canReplaySignupAttempt(attempt, signupPayload({ ...unchanged, pev_model: 'Another Wheel' }, 'en')), false);
  assert.equal(canReplaySignupAttempt(attempt, signupPayload(unchanged, 'es')), false);
});

test('removal requests send only normalized email, challenge, idempotency and honeypot fields', () => {
  const attempt = removalAttempt({
    email: ' rider@example.test ',
    idempotencyKey: 'removal-key',
    turnstileToken: 'challenge-token',
    website: ''
  });
  assert.equal(attempt.payload, removalPayload('rider@example.test'));
  assert.deepEqual(attempt.body, {
    idempotency_key: 'removal-key',
    email: 'rider@example.test',
    turnstile_token: 'challenge-token',
    website: ''
  });
});

test('a stalled signup request is aborted at its deadline', async () => {
  let observedSignal;
  await assert.rejects(fetchWithTimeout((_url, options) => new Promise((_resolve, reject) => {
    observedSignal = options.signal;
    options.signal.addEventListener('abort', () => reject(new Error('aborted')), { once: true });
  }), '/v1/alpha-signup/requests', {}, 5), /aborted/);
  assert.equal(observedSignal.aborted, true);
});

test('callbacks from a removed Turnstile widget generation are rejected', () => {
  assert.equal(isCurrentWidgetGeneration(8, 8), true);
  assert.equal(isCurrentWidgetGeneration(7, 8), false);
});
