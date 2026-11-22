import React from "react";
import "../Login.css";
import { login } from "../api/login";

interface LoginFormProps {
	setToken: (token: string) => void;
}

export const LoginForm: React.FC<LoginFormProps> = ({ setToken }) => {
	const [email, setEmail] = React.useState<string>("");
	const [password, setPassword] = React.useState<string>("");
	const [error, setError] = React.useState<string | null>(null);
	const submitLogin = (event: React.FormEvent) => {
		event.preventDefault();
		login(email, password)
			.then((response) => {
				setError(null);
				setToken(response);
			})
			.catch((error) => {
				setError("Login failed. Check your email and password, then try again.");
			});
	};
	const handlePasswordChange = (e: React.ChangeEvent<HTMLInputElement>) => {
		setPassword(e.target.value);
	};
	const handleUsernameChange = (e: React.ChangeEvent<HTMLInputElement>) => {
		setEmail(e.target.value);
	};
	return (
		<form className="login" onSubmit={submitLogin}>
			<h1 className="login-title">Login</h1>
			{error && <p className="appError" role="alert">{error}</p>}
			<input
				type="email"
				autoComplete="username"
				className="login-input"
				placeholder="Email"
				autoFocus
				onChange={handleUsernameChange}
				value={email}
			/>
			<input
				type="password"
				autoComplete="current-password"
				className="login-input"
				placeholder="Password"
				onChange={handlePasswordChange}
				value={password}
			/>
			<button className="login-button" id="login-button" type="submit">
				Let's go
			</button>
		</form>
	);
};
