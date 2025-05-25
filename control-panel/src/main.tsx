import React from 'react';
import dom from 'react-dom/client';
import Router, {Link, Redirect, Route} from "./router.js";
import ListDatabases from "./list-databases.js";

export default function main(root: HTMLElement) {
    dom.createRoot(root)
        .render(<Router base={"/portal"} else={<span>{"404"}</span>}>
            <Route matcher={"/portal/index.html"}>
                <Portal/>
            </Route>
            <Route matcher={"/portal/login.html"}>
                <LoginPage/>
            </Route>
        </Router>);
}

export function Portal(props: {}) {
    const user = window.localStorage.getItem('user');

    if (!user)
        return <>
            <h1>{"You need to log in first"}</h1>
            <Link to={"/portal/login.html"}>{'Login'}</Link>
        </>;

    return <>
        <h1>{`Hello ${user}`}</h1>

        <ListDatabases/>
    </>;
}

export function LoginPage(props: {}) {
    const query = new URLSearchParams(window.location.search);
    const [url, setUrl] = React.useState<URL | null>(null);

    React.useEffect(() => {
        fetch('https://db.es03/oauth')
            .then(res => res.json())
            .then((oauth: { authorisation: string, redirect: string, client_id: string }) => {
                const url = new URL(oauth.authorisation);
                url.searchParams.append('response_type', 'code');
                url.searchParams.append('client_id', oauth.client_id);
                url.searchParams.append('redirect_uri', oauth.redirect);
                return url;
            })
            .then(url => setUrl(url));

    }, []);

    if (url == null)
        return <span>{"Please wait"}</span>;
    else if (query.has('code'))
        return <OAuthResponse/>
    else {
        const token = window.localStorage.getItem('token');

        if (token)
            return <Redirect to={"/portal/index.html"}/>;

        return <>
            <h1>{"Login"}</h1>
            <a href={url.href}>{"Log in"}</a>
        </>;
    }
}

export function OAuthResponse(props: {}) {
    const query = new URLSearchParams(window.location.search);
    const [token, setToken] = React.useState<{
        token: string,
        refresh: string,
        user: string,
        expires_in: number
    } | null>(null);

    fetch('https://db.es03/oauth', {
        method: 'POST',
        headers: {
            'Content-Type': 'application/json'
        },
        body: JSON.stringify({
            code: query.get('code')
        })
    }).then(res => res.json())
        .then(token => {
            window.localStorage.setItem('token', token.token);
            window.localStorage.setItem('user', token.user);
            window.localStorage.setItem('expiry', String(Date.now() + 1000 * token.expires_in));
            window.localStorage.setItem('refresh', token.refresh);
            setToken(token);
        })

    if (token == null)
        return <span>{"Please wait"}</span>;
    else if ('token' in token)
        return <Redirect to={"/portal/index.html"}/>;
    else
        return <span>{"Logging in"}</span>;
}