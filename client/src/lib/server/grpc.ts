import protoLoader from "@grpc/proto-loader";
import {
    ChannelCredentials,
    credentials,
    loadPackageDefinition,
} from "@grpc/grpc-js";
import type { ProtoGrpcType } from "$lib/proto/main";
import { ENV, USERS_URI, NOTES_URI, UTILS_URI } from "$env/static/private";

export const packageDefinition = protoLoader.loadSync(
    "./src/lib/proto/main.proto",
    {
        keepCase: false,
        longs: String,
        defaults: true,
        oneofs: true,
        arrays: true,
        objects: true,
    },
);
export const proto = loadPackageDefinition(
    packageDefinition,
) as unknown as ProtoGrpcType;

const cr: ChannelCredentials =
    ENV === "production"
        ? credentials.createSsl()
        : credentials.createInsecure();

export const usersService = new proto.proto.UsersService(USERS_URI, cr);

export const notesService = new proto.proto.NotesService(NOTES_URI, cr);

export const utilsService = new proto.proto.UtilsService(UTILS_URI, cr);
