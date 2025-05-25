import React from 'react';

import ApiResource, { Response } from "./network-resource.js";

export default function ListDatabases() {
    return <ApiResource<{  }> apiResource={{ url: '/databases' }}>
        <Table />
    </ApiResource>;
}

export function Table(props: Response<{}>) {
    console.log('response' in props ? props.response : 'loading');

    if ('response' in props)
        return <pre>
            {JSON.stringify(props.response, null, 4)}
        </pre>;
    else
        return <span>{"Loading"}</span>;
}