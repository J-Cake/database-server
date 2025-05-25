import React from 'react';

export function Route(props: React.PropsWithChildren<{ matcher: string }>) {
    return <>{props.children}</>;
}

export function Redirect(props: { to: string }) {
    const navigator = React.useContext(nav);
    navigator.navigate(props.to);
    return <></>
}

export function Link(props: React.PropsWithChildren<{ to: string }>) {
    const navigator = React.useContext(nav);

    function handleClick(e: React.MouseEvent<HTMLAnchorElement>) {
        e.preventDefault();
        navigator.navigate(props.to);
    }

    return <a href={props.to} onClick={e => handleClick(e)}>{props.children}</a>;
}

const nav = React.createContext<{ navigate(to: string): void }>({
    navigate(to: string) {
        throw new Error(`Cannot navigate to ${to} outside of a router`);
    }
});

export default function Router(props: { base?: string, children: React.ReactElement<{ matcher: string }> | React.ReactElement<{ matcher: string }>[], else?: React.ReactElement }) {
    const [href, setUrl] = React.useState(window.location.href);
    const url = React.useMemo(() => new URL(href).href, [href]);
    const base = props.base ? new URL(props.base, window.location.origin) : new URL(window.location.host);

    function navigate(to: string) {
        setTimeout(() => {
            const url = new URL(to, window.location.href).href
            window.history.pushState(url, '', url);
            setUrl(url);
        });
    }

    window.addEventListener('popstate', e => {
        navigate(window.location.href)
    });

    for (const child of [props.children].flat()) {
        const pattern = new URLPattern(child.props.matcher, base.href);

        if (pattern.test(url))
            return <nav.Provider key={child.props.matcher} value={{ navigate: url => navigate(url) }}>
                {child}
        </nav.Provider>;
    }

    return <>{props.else}</>;
}