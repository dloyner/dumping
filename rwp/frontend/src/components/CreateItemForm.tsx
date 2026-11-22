import type { ChangeEvent, FC } from "react";
import { useState } from "react";
import { createToDoItemCall } from "../api/create";
import type { ApiResult } from "../api/utils";
import type { ToDoItems } from "../interfaces/toDoItems";

interface CreateToDoItemProps {
	passBackResponse: (response: ApiResult<ToDoItems>) => void;
}

export const CreateToDoItem: FC<CreateToDoItemProps> = ({
	passBackResponse,
}) => {
	const [title, setTitle] = useState<string>("");
	const [busy, setBusy] = useState(false);

	const handleTitleChange = (e: ChangeEvent<HTMLInputElement>) => {
		setTitle(e.target.value);
	};

	return (
		<form className="inputContainer" onSubmit={async (e) => {
			e.preventDefault();
			if (busy || !title.trim()) return;
			setBusy(true);
			try {
				const response = await createToDoItemCall(title.trim());
				if (!("error" in response)) setTitle("");
				passBackResponse(response);
			} finally { setBusy(false); }
		}}>
			<input
				type="text"
				id="title"
				placeholder="create to do item"
				value={title}
				onChange={handleTitleChange}
			/>
			<button className="actionButton" id="create-button" disabled={busy}>
				Create
			</button>
		</form>
	);
};
