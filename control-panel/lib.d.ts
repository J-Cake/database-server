declare global {
    interface URLPatternOptions {
        ignoreCase?: boolean
    }

    interface URLPatternMatchResult {
        inputs: string,
        groups: Record<keyof object, string>
    }

    interface URLParts {
        protocol?: string,
        username?: string,
        password?: string,
        hostname?: string,
        port?: string,
        pathname?: string,
        search?: string,
        hash?: string,
    }

    class URLPattern {
        constructor(input: string | URLParts, base?: string, options?: URLPatternOptions);

        get hash(): string;

        get hostname(): string;

        get password(): string;

        get pathname(): string;

        get port(): string;

        get protocol(): string;

        get search(): string;

        get username(): string;

        exec(input: string, baseURL?: string): URLPatternMatchResult | null;
        exec(input: URLParts, baseURL?: string): object | null;
        exec(input: string | URLParts, baseURL?: string): URLPatternMatchResult | null;

        test(input: string | URLParts, baseURL?: string): boolean;
    }

    interface Window {
        URLPattern: URLPattern;
    }
}

export {}