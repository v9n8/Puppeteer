// "Resend" (V3-28): the "Edit and resend" flow with the edit sent untouched.
// A declined or failed rewind sends nothing and puts the old draft back; once
// the conversation has been rewound the edit stays, so the text is not lost.
export async function resendEdit<E extends { rewound: boolean }>(
  editing: { readonly value: E | null },
  send: (edit: E) => Promise<void>,
  cancel: () => void,
): Promise<void> {
  const edit = editing.value
  if (!edit) return
  await send(edit)
  if (editing.value === edit && !edit.rewound) cancel()
}
