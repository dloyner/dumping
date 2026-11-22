import type { TaskStatus, ToDoItems } from "../interfaces/toDoItems";
import { Url } from "./url";
import { patchCall } from "./utils";

export async function updateToDoItemCall(
	id: number,
	status: TaskStatus,
) {
	return patchCall<ToDoItems, { id: number; status: TaskStatus }>(new Url().update, { id, status }, 200);
}
