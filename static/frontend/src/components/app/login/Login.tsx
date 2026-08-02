import { useRef, useState } from "react"
import "../login/Login.css"

export interface LoginProps {

}

export function Login({ }: LoginProps) {
    const [showHostPasswordField, setShowHostPasswordField] = useState(false);
    const [createLobbyRequest, setCreateLobbyRequest] = useState({
        lobbyId: "",
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
    function toggleShowPasswords(e: React.ChangeEvent<HTMLInputElement>) {
        const type = e.target.checked ? "text" : "password";
        lobbyPasswordInputRef.current!.type = type;
        hostPasswordInputRef.current!.type = type;
    }

    return (
        <div className="login">
            <h1 className="login-title">Jeopardy</h1>
            <form className="login-form" onSubmit={handleLoginDialog}>
                <div className="login-error-msg">Error during login.</div>
                <label>
                    Username
                    <input className="login-text-input" type="text" name="username" placeholder="Username" value={createLobbyRequest.username} onChange={handleInput} />
                </label>
                <label>
                    Lobby Name
                    <input className="login-text-input" type="text" name="lobbyId" placeholder="Lobby Name" value={createLobbyRequest.lobbyId} onChange={handleInput} />
                </label>
                <label>
                    Lobby Password
                    <input className="login-text-input" ref={lobbyPasswordInputRef} type="password" placeholder="Lobby Password" name="lobbyPassword" value={createLobbyRequest.lobbyPassword} onChange={handleInput} />
                    <label>
                        <input type="checkbox" onChange={toggleShowPasswords}></input>
                        Show Password
                    </label>
                    <br />
                </label>
                <label hidden={!showHostPasswordField} >
                    Host Password
                    <input className="login-text-input" ref={hostPasswordInputRef} type="password" name="hostPassword" placeholder="Host Password" value={createLobbyRequest.hostPassword} onChange={handleInput} />
                </label>
                <input type="submit" name="join-lobby" value="Join Lobby" onClick={() => {
                    setShowHostPasswordField(false);
                }} />
                <input type="submit" name="create-lobby" value="Create Lobby" onClick={() => {
                    if (!showHostPasswordField) {
                        setShowHostPasswordField(true);
                    }
                }} />
            </form>
        </div >
    )
}

function handleLoginDialog(e: React.SubmitEvent) {
    e.preventDefault();
}