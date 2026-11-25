export type SidebarUnreadBadgeProps = {
  count?: number | string;
};

export default function SidebarUnreadBadge({ count = 0 }: SidebarUnreadBadgeProps) {
  const num = Number(count);
  const normalizedCount = Number.isFinite(num) && num > 0 ? num : 0;

  if (normalizedCount === 0) {
    return <span className="hidden" />;
  }

  const displayCount = normalizedCount > 99 ? "99+" : String(normalizedCount);

  return (
    <span
      data-test-id="sidebar-unread-badge"
      className="inline-grid h-5 min-w-5 place-items-center rounded-full bg-n-slate-4 px-1 text-xxs font-medium leading-3 text-n-slate-12 dark:bg-n-slate-5 shrink-0"
    >
      {displayCount}
    </span>
  );
}
