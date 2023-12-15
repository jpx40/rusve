import { Metadata } from "@grpc/grpc-js";
import jwt from "jsonwebtoken";
import fs from "fs";

/**
 * Load the private key from the file system
 */
const key = fs.readFileSync("./src/lib/server/private.key");

/**
 * Create a Metadata object with the correct authorization headers
 * @param {string} token - The token id to use for the authorization
 * @param {string} userId - The user id to use for the authorization
 * @returns {Metadata} - Metadata object with the correct authorization headers
 */
export function createMetadata(token, userId) {
    const metadata = new Metadata();

    const tokenPayload = {
        token: token,
        user_id: userId,
    };

    // Generate and sign the token
    const oauthToken = jwt.sign(tokenPayload, key, {
        algorithm: "RS256",
        expiresIn: "1h",
    });

    metadata.set("x-authorization", `bearer ${oauthToken}`);
    return metadata;
}
