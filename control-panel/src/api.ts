// api.ts
export interface RefreshResponse {
    token: string;
    refresh: string;
    expires_in: number;      // seconds until token expiration
}

async function doRefresh(): Promise<void> {
    const refreshToken = window.localStorage.getItem('refresh');
    if (!refreshToken) {
        throw new Error('No refresh token available, please log in again.');
    }

    const res = await fetch('/refresh', {
        method: 'POST',
        headers: {'Content-Type': 'application/json'},
        body: JSON.stringify({refresh: refreshToken}),
    });

    if (!res.ok) {
        throw new Error('Refresh token request failed');
    }

    const data: RefreshResponse = await res.json();
    const expiresAt = Date.now() + data.expires_in * 1000;

    window.localStorage.setItem('token', data.token);
    window.localStorage.setItem('refresh', data.refresh);
    window.localStorage.setItem('expiry', expiresAt.toString());
}

function isExpired(): boolean {
    const expiry = window.localStorage.getItem('expiry');
    if (!expiry) return true;
    return Date.now() > parseInt(expiry, 10);
}

/**
 * A drop‐in replacement for fetch. Automatically ensures
 * you have a valid access token, and retries once on 401.
 */
export async function fetchWithAuth(input: RequestInfo, init?: RequestInit): Promise<Response>;
export async function fetchWithAuth(input: URL, init?: RequestInit): Promise<Response>;
export async function fetchWithAuth(input: string, init?: RequestInit): Promise<Response>;
export async function fetchWithAuth(
    input: RequestInfo | URL | string,
    init?: RequestInit
): Promise<Response> {
    // Normalize init to an object so we can safely read/clone headers, body, etc.
    const initOptions: RequestInit = {...init};

    // 1) Refresh if expired
    if (isExpired()) {
        await doRefresh();
    }

    // 2) Attach bearer token
    const token = window.localStorage.getItem('token') ?? '';
    const headers = new Headers(initOptions.headers);
    headers.set('Authorization', `Bearer ${token}`);
    headers.set('Accept', 'application/json');
    if (initOptions.body && !(initOptions.body instanceof FormData)) {
        headers.set('Content-Type', 'application/json');
    }

    // First attempt
    let response = await fetch(input, {...initOptions, headers});

    // 3) If server rejects with 401, try ONE more refresh + retry
    if (response.status === 401) {
        await doRefresh();

        const newToken = window.localStorage.getItem('token') ?? '';
        headers.set('Authorization', `Bearer ${newToken}`);
        response = await fetch(input, {...initOptions, headers});
    }

    return response;
}