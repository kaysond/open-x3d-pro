// crypto.randomUUID only exists in secure contexts; the dev server is also opened over plain-HTTP LAN IPs.
export const uuid = (): string =>
  '10000000-1000-4000-8000-100000000000'.replace(/[018]/g, (c) =>
    (+c ^ ((crypto.getRandomValues(new Uint8Array(1))[0] ?? 0) & (15 >> (+c / 4)))).toString(16),
  );
