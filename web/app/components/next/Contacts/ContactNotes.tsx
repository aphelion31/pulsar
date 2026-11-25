import { useState, useEffect, useCallback } from "react";
import Button from "~/components/next/Button";
import TextArea from "~/components/next/TextArea";
import Spinner from "~/components/next/Spinner";
import Avatar from "~/components/next/Avatar";
import { api, acct } from "~/lib/api";
import { timeAgo } from "~/lib/format";
import { t } from "~/lib/i18n";

export type NoteItem = {
  id: number;
  content: string;
  account_id: number;
  contact_id: number;
  created_at: number;
  updated_at: number;
  user?: {
    id: number;
    name: string;
    email: string;
    avatar_url?: string;
  };
};

type Props = {
  accountId: number;
  contactId: number;
  currentUserId?: number;
};

export default function ContactNotes({
  accountId,
  contactId,
  currentUserId,
}: Props) {
  const [notes, setNotes] = useState<NoteItem[]>([]);
  const [content, setContent] = useState("");
  const [isLoading, setIsLoading] = useState(true);
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [deletingId, setDeletingId] = useState<number | null>(null);
  const [error, setError] = useState("");

  const base = `${acct(accountId)}/contacts/${contactId}/notes`;

  const fetchNotes = useCallback(async () => {
    setIsLoading(true);
    try {
      const res = await api<NoteItem[] | { payload: NoteItem[] }>(base);
      setNotes(Array.isArray(res) ? res : res.payload ?? []);
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : "Failed to load notes");
    } finally {
      setIsLoading(false);
    }
  }, [base]);

  useEffect(() => {
    fetchNotes();
  }, [fetchNotes]);

  const handleAddNote = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!content.trim()) return;

    setIsSubmitting(true);
    setError("");
    try {
      await api(base, {
        method: "POST",
        json: { content: content.trim() },
      });
      setContent("");
      await fetchNotes();
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : "Failed to save note");
    } finally {
      setIsSubmitting(false);
    }
  };

  const handleDeleteNote = async (noteId: number) => {
    setDeletingId(noteId);
    try {
      await api(`${base}/${noteId}`, { method: "DELETE" });
      setNotes((prev) => prev.filter((n) => n.id !== noteId));
    } catch (err: unknown) {
      alert(err instanceof Error ? err.message : "Failed to delete note");
    } finally {
      setDeletingId(null);
    }
  };

  return (
    <div className="flex flex-col gap-6 px-6">
      <form onSubmit={handleAddNote} className="flex flex-col gap-2">
        <TextArea
          placeholder={t("CONTACTS_LAYOUT.SIDEBAR.NOTES.PLACEHOLDER")}
          modelValue={content}
          onModelValueChange={(v) => setContent(v)}
          autoHeight
          maxHeight="8rem"
        />
        <div className="flex justify-end">
          <Button
            type="submit"
            size="sm"
            color="blue"
            variant="solid"
            label={t("CONTACTS_LAYOUT.SIDEBAR.NOTES.SAVE")}
            isLoading={isSubmitting}
            disabled={isSubmitting || !content.trim()}
          />
        </div>
      </form>

      {error && (
        <div className="rounded-lg bg-n-ruby-9/10 border border-n-ruby-8/30 p-3 text-xs text-n-ruby-11">
          {error}
        </div>
      )}

      {isLoading ? (
        <div className="flex items-center justify-center py-8 text-n-slate-11">
          <Spinner size={24} />
        </div>
      ) : notes.length === 0 ? (
        <p className="py-8 text-sm text-center text-n-slate-11">
          {t("CONTACTS_LAYOUT.SIDEBAR.NOTES.EMPTY_STATE")}
        </p>
      ) : (
        <div className="flex flex-col gap-3">
          {notes.map((note) => {
            const author =
              note.user?.id === currentUserId
                ? t("CONTACTS_LAYOUT.SIDEBAR.NOTES.YOU")
                : note.user?.name || "Agent";
            return (
              <div
                key={note.id}
                className="flex flex-col gap-2 rounded-xl border border-n-weak bg-n-surface-1 p-3.5 shadow-2xs"
              >
                <div className="flex items-center justify-between gap-2">
                  <div className="flex items-center gap-2">
                    <Avatar
                      name={note.user?.name || author}
                      size={20}
                      hideOfflineStatus
                    />
                    <span className="text-xs font-medium text-n-slate-12">
                      {author}
                    </span>
                    <span className="text-xs text-n-slate-10">
                      {timeAgo(note.created_at)}
                    </span>
                  </div>
                  <Button
                    icon="i-lucide-trash"
                    variant="ghost"
                    color="slate"
                    size="xs"
                    className="p-1 hover:text-n-ruby-11"
                    isLoading={deletingId === note.id}
                    disabled={deletingId === note.id}
                    onClick={() => handleDeleteNote(note.id)}
                    aria-label="Delete note"
                  />
                </div>
                <p className="text-sm whitespace-pre-wrap text-n-slate-11">
                  {note.content}
                </p>
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}
