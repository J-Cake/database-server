import React from 'react';

import ApiResource, {Response} from "./network-resource.js";
import {Link} from "./router.js";

interface Database {
    name: string,
    id: DatabaseID,
    owner: UserID,
    rw: UserID[],
    ro: UserID[]
}

export type DatabaseID = string;
export type UserID = string;

export default function ListDatabases() {
    return <ApiResource<{}> apiResource={{url: '/databases'}}>
        <Table/>
    </ApiResource>;
}

export function Table(props: Response<{ success: boolean, databases: Database[] }>) {
    console.log('response' in props ? props.response : 'loading');

    if ('response' in props)
        return <div>
            <table>
                <thead>
                <tr>
                    <th>{"Name"}</th>
                    <th>{"Objects"}</th>
                    <th>{"Owner"}</th>
                    <th>{"RW"}</th>
                    <th>{"RO"}</th>
                </tr>
                </thead>
                <tbody>
                {props.response.databases.map((i, a) => <tr key={`database-${i.id}`}>
                    <td>{i.name}</td>
                    <td><i>{"Huge, probably."}</i></td>
                    <td>{i.owner}</td>
                    <td>
                        <ul>{i.rw.map(i => <li>{i}</li>)}</ul>
                    </td>
                    <td>
                        <ul>{i.ro.map(i => <li>{i}</li>)}</ul>
                    </td>
                </tr>)}
                </tbody>
            </table>
            <Link to={"/portal/create-database.html"}>{ "Create Database" }</Link>
        </div>;
    else
        return <span>{"Loading"}</span>;
}