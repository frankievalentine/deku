// API client for the dekud REST API and WebSocket event stream.

const BASE_URL = import.meta.env.PUBLIC_API_URL ?? '';

export async function getApps() {
  const res = await fetch(`${BASE_URL}/api/apps`);
  if (!res.ok) throw new Error(`GET /api/apps failed: ${res.status}`);
  return res.json();
}

export async function createApp(name: string) {
  const res = await fetch(`${BASE_URL}/api/apps`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ name }),
  });
  if (!res.ok) throw new Error(`POST /api/apps failed: ${res.status}`);
  return res.json();
}

export async function deleteApp(name: string) {
  const res = await fetch(`${BASE_URL}/api/apps/${name}`, { method: 'DELETE' });
  if (!res.ok) throw new Error(`DELETE /api/apps/${name} failed: ${res.status}`);
}
