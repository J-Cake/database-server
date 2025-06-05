// network-resource.tsx
import React from 'react'
import {fetchWithAuth} from './api.js';

export type Response<T> = { response: T } | {}

export interface ApiResource {
    method?: 'GET' | 'POST' | 'PUT' | 'DELETE'
    url: string | URL
    body?: string
}

export default function ApiResource<T>(props: {
    children: React.ReactElement<Response<T>>
    apiResource: ApiResource
}) {
    const {url, method = 'GET', body} = props.apiResource
    const [result, setResult] = React.useState<Response<T>>({})

    React.useEffect(() => {
        let active = true

        async function load() {
            try {
                const res = await fetchWithAuth(url instanceof URL ? url : new URL(url, window.location.origin), {method, body})
                if (!res.ok) throw new Error(`Request failed ${res.status}`);

                const contentType = res.headers.get('Content-Type') || ''
                let payload: any

                if (contentType.includes('application/json')) {
                    payload = await res.json()
                } else if (contentType.includes('text/plain')) {
                    payload = await res.text()
                } else {
                    payload = await res.blob()
                }

                if (active) {
                    setResult({response: payload as T})
                }
            } catch (err) {
                if (active) {
                    console.error('API error:', err)
                }
            }
        }

        load()
        return () => {
            active = false
        }
    }, [url, method, body])

    return React.cloneElement(props.children, result)
}