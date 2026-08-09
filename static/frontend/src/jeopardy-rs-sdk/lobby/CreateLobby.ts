import type { CreateLobbyRequest } from "../generated/CreateLobbyRequest";
import type { CreateLobbyResponse } from "../generated/CreateLobbyResponse";

const CREATE_LOBBY_PATH = "/lobbies"

export async function createLobby(request: CreateLobbyRequest): Promise<CreateLobbyResponse> {
    const rawResponse = await fetch(CREATE_LOBBY_PATH, {
        method: 'POST',
        headers: { "Content-Type": 'application/json' },
        body: JSON.stringify(request)
    });
    // TODO: error handling
    return await rawResponse.json();
}