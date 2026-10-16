export function signupPayload(values, locale) {
  return JSON.stringify({
    email: String(values.email || '').trim(),
    pev_type: String(values.pev_type || '').trim() || null,
    pev_model: String(values.pev_model || '').trim(),
    requested_accessories: String(values.requested_accessories || '').trim(),
    locale
  });
}

export function signupAttempt({ values, locale, idempotencyKey, turnstileToken, website = '' }) {
  const payload = signupPayload(values, locale);
  const normalized = JSON.parse(payload);
  return {
    payload,
    body: {
      idempotency_key: idempotencyKey,
      ...normalized,
      turnstile_token: turnstileToken,
      website
    }
  };
}

export function canReplaySignupAttempt(attempt, payload) {
  return Boolean(attempt && attempt.payload === payload);
}

export function isCurrentWidgetGeneration(callbackGeneration, activeGeneration) {
  return callbackGeneration === activeGeneration;
}

export function removalPayload(email) {
  return JSON.stringify({ email: String(email || '').trim() });
}

export function removalAttempt({ email, idempotencyKey, turnstileToken, website = '' }) {
  return {
    payload: removalPayload(email),
    body: {
      idempotency_key: idempotencyKey,
      email: String(email || '').trim(),
      turnstile_token: turnstileToken,
      website
    }
  };
}

export async function fetchWithTimeout(fetcher, url, options, timeoutMs = 12000) {
  const controller = new AbortController();
  const timeout = setTimeout(() => controller.abort(), timeoutMs);
  try {
    return await fetcher(url, { ...options, signal: controller.signal });
  } finally {
    clearTimeout(timeout);
  }
}
