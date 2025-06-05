import React, {FormEvent} from 'react';
import {fetchWithAuth} from "./api.js";

export interface CreateDatabaseProps {

}

export default function CreateDatabasePage(props: CreateDatabaseProps) {
    return <form onSubmit={e => createDatabase(e)}>
        <div>
            <label htmlFor="database-name">{"The name of the database"}</label>
            <input type="text" name="name" placeholder="Database Name" id="database-name"
                   title="This name is not used to identify the database, but may be useful to you."/>
        </div>

        <button>{"Create Database"}</button>
    </form>;
}

export function createDatabase(e: FormEvent<HTMLFormElement>) {
    e.preventDefault();

    const url = new URL("/databases", 'https://db.es03')
    for (const [key, value] of new FormData(e.currentTarget))
        if (value instanceof File)
            console.warn('File not supported');
        else
            url.searchParams.append(key, value);

    fetchWithAuth(url, {
        method: 'PUT'
    }).then(res => res.json())
        .then(res => console.log(res))
        .then(_ => window.history.back());
}