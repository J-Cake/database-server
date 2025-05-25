import React from 'react';

export type Response<T> = {
    response: T
} | {};

export default function ApiResource<T>(props: {
    children: React.ReactElement<Response<T>>,
    apiResource: ApiResource
}) {
    const [response, setResponse] = React.useState<{ response: T } | {}>({});

    React.useEffect(() => {
        fetch(props.apiResource.url, {
            method: props.apiResource.method ?? 'GET',
            body: props.apiResource.body,
            headers: {
                'Content-Type': 'application/json',
                'Accept': `application/json; charset=utf-8, text/plain; charset=utf-8, application/octet-stream`,
                'Authorization': `Bearer ${window.localStorage.getItem('token')}`
            }
        })
            .then(async res => {
                if (!res.ok && res.status === 401 && new Date(Number(window.localStorage.getItem('expiry') ?? '0')) < new Date())
                    return await fetch('https://db.es03/refresh', {
                        method: 'POST',
                        headers: {'Content-Type': 'application/json'},
                        body: JSON.stringify({
                            refresh: window.localStorage.getItem('refresh') ?? ''
                        })
                    })
                        .then(res => res.json())
                        .then(refresh => {
                            window.localStorage.setItem('token', refresh.token);
                            window.localStorage.setItem('expiry', String(Date.now() + 1000 * refresh.expires_in));
                            window.localStorage.setItem('refresh', refresh.refresh);
                        })
                        .then(() => fetch(props.apiResource.url, {
                            method: props.apiResource.method ?? 'GET',
                            body: props.apiResource.body,
                            headers: {
                                'Content-Type': 'application/json',
                                'Accept': `application/json; charset=utf-8, text/plain; charset=utf-8, application/octet-stream`,
                                'Authorization': `Bearer ${window.localStorage.getItem('token')}`
                            }
                        }));
                else
                    return res;
            })
            .then(res => {
                if (res.headers.get('Content-Type')?.startsWith('application/json'))
                    return res.json();
                else if (res.headers.get('Content-Type')?.startsWith('text/plain'))
                    return res.text();
                else
                    return res.blob();
            }).then(res => setResponse({response: res}));
    }, []);

    return React.cloneElement(props.children, response);
}

export interface ApiResource {
    method?: 'GET' | 'POST' | 'PUT' | 'DELETE',
    url: string | URL,
    body?: string,
}