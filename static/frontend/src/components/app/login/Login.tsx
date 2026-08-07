import { useRef, useState } from "react"
import { createLobby } from "../../../jeopardy-rs-sdk/lobby/CreateLobby";
import { joinLobby } from "../../../jeopardy-rs-sdk/lobby/JoinLobby";
import "./Login.css"

// dummy data for the CreateLobbyRequest
const TEST_JEOPARDY_CONFIG = {
    "boards": [
        {
            "categories": [
                {
                    "name": "test",
                    "questions": [
                        {
                            "pointValue": 0,
                            "dailyDouble": false,
                            "answered": false,
                            "question": {
                                "content": "test_content",
                                "answer": "test_answer"
                            }
                        }
                    ]
                }
            ]
        }
    ],
    "finalJeopardy": {
        "hint": "hint",
        "question": {
            "content": "final_jeopardy",
            "answer": "answer"
        }
    }
};

export interface LoginProps {

}

export function Login({ }: LoginProps) {
    const [showHostPasswordField, setShowHostPasswordField] = useState(false);
    const [createLobbyRequest, setCreateLobbyRequest] = useState({
        lobbyName: "",
        lobbyPassword: "",
        hostPassword: "",
        username: ""
    });

    // simple handler that maps the form input to the internal react object
    // TODO: since this updates on each key press, we can provide helpful validation hints
    function handleInput(
        e: React.ChangeEvent<HTMLInputElement>
    ) {
        const { name, value } = e.target;
        setCreateLobbyRequest(prev => ({
            ...prev,
            [name]: value
        }));
    }

    // show/hide password checkbox
    const lobbyPasswordInputRef = useRef<HTMLInputElement>(null);
    const hostPasswordInputRef = useRef<HTMLInputElement>(null);
    const loginResponseRef = useRef<HTMLInputElement>(null);
    function toggleShowPasswords(e: React.ChangeEvent<HTMLInputElement>) {
        const type = e.target.checked ? "text" : "password";
        lobbyPasswordInputRef.current!.type = type;
        hostPasswordInputRef.current!.type = type;
    }

    return (
        <div className="login">
            <h1 className="login-title">Jeopardy</h1>
            <form className="login-form" onSubmit={e => e.preventDefault()}>
                <div ref={loginResponseRef} className="login-response"></div>
                <label>
                    Username
                    <input required={true}
                        className="login-text-input"
                        type="text"
                        name="username"
                        placeholder="Username"
                        value={createLobbyRequest.username}
                        onChange={handleInput}
                    />
                </label>
                <label>
                    Lobby Name
                    <input required={true}
                        className="login-text-input"
                        type="text"
                        name="lobbyName"
                        placeholder="Lobby Name"
                        value={createLobbyRequest.lobbyName}
                        onChange={handleInput}
                    />
                </label>
                <label>
                    Lobby Password
                    <input required={true}
                        className="login-text-input"
                        ref={lobbyPasswordInputRef}
                        type="password"
                        placeholder="Lobby Password"
                        name="lobbyPassword"
                        value={createLobbyRequest.lobbyPassword}
                        onChange={handleInput}
                    />
                    <label>
                        <input
                            type="checkbox"
                            onChange={toggleShowPasswords}
                        />
                        Show Password
                    </label>
                    <br />
                </label>
                <label hidden={!showHostPasswordField} >
                    Host Password
                    <input required={showHostPasswordField}
                        className="login-text-input"
                        ref={hostPasswordInputRef}
                        type="password"
                        name="hostPassword"
                        placeholder="Host Password"
                        value={createLobbyRequest.hostPassword}
                        onChange={handleInput}
                    />
                </label>
                <input type="submit"
                    name="join-lobby"
                    value="Join Lobby"
                    onClick={async () => {
                        setShowHostPasswordField(false);
                        try {
                            const playerConn = await joinLobby({
                                ...createLobbyRequest,
                                lobbyId: createLobbyRequest.lobbyName,
                            });
                        } catch (e: unknown) {
                            const loginResponseElement = loginResponseRef.current!;
                            loginResponseElement.style.color = `red`;
                            loginResponseElement.textContent = `Login failed: ` +
                                ((e instanceof Error)
                                    ? e.message
                                    : `Generic failure`);
                        }
                    }}
                />
                <input type="submit"
                    name="create-lobby"
                    value="Create Lobby"
                    onClick={async () => {
                        if (!showHostPasswordField) {
                            setShowHostPasswordField(true);
                        } else {
                            const { requestId, error } = await createLobby({
                                ...createLobbyRequest,
                                config: TEST_JEOPARDY_CONFIG
                            });
                            const loginResponse = loginResponseRef.current!;
                            if (error) {
                                loginResponse.style = `color: red`;
                                loginResponse.textContent = `${error} (requestId: ${requestId})`;
                            } else {
                                loginResponse.style = `color: green`;
                                loginResponse.textContent = `Lobby successfully created`;
                            }
                        }
                    }} />
            </form>
        </div>
    )
}