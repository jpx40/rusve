import { COOKIE_DOMAIN } from "$env/static/private";
import { grpcSafe } from "$lib/safe";
import { usersService } from "$lib/server/grpc";
import { logger, perf } from "$lib/server/logger";
import { createMetadata } from "$lib/server/metadata";
import { Metadata } from "@grpc/grpc-js";
import { redirect } from "@sveltejs/kit";

/** @type {import('@sveltejs/kit').Handle} */
export async function handle({ event, resolve }) {
    const end = perf("auth");
    event.locals.user = {
        id: "",
        created: "",
        updated: "",
        deleted: "infinity",
        email: "",
        sub: "",
        role: 0,
        avatar: "",
        subscription_id: "",
        subscription_end: "-infinity",
        subscription_check: "-infinity",
        subscription_active: false,
    };

    if (event.url.pathname === "/auth") {
        event.cookies.set("token", "", {
            domain: COOKIE_DOMAIN,
            path: "/",
            maxAge: 0,
        });
        return await resolve(event);
    }

    /**
     * Check if the user is coming from the oauth flow
     * If so, send the token to the backend to create a user
     */
    const oauth_token = event.url.searchParams.get("oauth_token");
    if (oauth_token) {
        const metadata = new Metadata();
        metadata.add("x-authorization", `bearer ${oauth_token}`);
        /** @type {import("$lib/safe").Safe<import("$lib/proto/proto/Id").Id__Output>} */
        const token = await new Promise((res) => {
            usersService.CreateUser({}, metadata, grpcSafe(res));
        });
        if (token.error) {
            throw redirect(302, "/auth?error=1");
        }
        event.cookies.set("token", token.data.id, {
            domain: COOKIE_DOMAIN,
            path: "/",
            // 10 seconds, it's just to create the user and redirect to the dashboard
            // after that the token will be refreshed with 7 days max age
            maxAge: 10,
        });
        throw redirect(302, "/dashboard");
    }

    if (event.url.pathname === "/") {
        throw redirect(302, "/dashboard");
    }

    const token = event.cookies.get("token") ?? "";
    if (!token) {
        logger.info("No token");
        throw redirect(302, "/auth");
    }

    const metadata = createMetadata(token);
    /** @type {import("$lib/safe").Safe<import("$lib/proto/proto/AuthResponse").AuthResponse__Output>} */
    const auth = await new Promise((res) => {
        usersService.Auth({}, metadata, grpcSafe(res));
    });
    if (auth.error || !auth.data.token || !auth.data.user) {
        logger.error("Error during auth");
        throw redirect(302, "/auth?error=1");
    }
    logger.debug(auth.data.user);

    event.locals.user = auth.data.user;
    event.locals.token = auth.data.token;

    end();
    const response = await resolve(event);
    // max age is 7 days
    response.headers.append(
        "set-cookie",
        `token=${auth.data.token}; HttpOnly; SameSite=Strict; Secure; Max-Age=604800; Domain=${COOKIE_DOMAIN}; Path=/`,
    );
    return response;
}
