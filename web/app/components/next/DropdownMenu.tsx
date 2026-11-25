import { useState } from "react";
import type { ReactNode } from "react";
import Avatar from "./Avatar";
import Icon from "./Icon";
import Spinner from "./Spinner";

type Item = { label: string; value?: unknown; action?: string; disabled?: boolean; isSelected?: boolean; thumbnail?: { name: string; src?: string }; icon?: string; emoji?: string; iconColor?: string };
type Section = { title?: string; items: Item[]; isLoading?: boolean; emptyState?: string };
type Props = { menuItems?: Item[]; menuSections?: Section[]; thumbnailSize?: number; roundedThumbnail?: boolean; showSearch?: boolean; searchPlaceholder?: string; isSearching?: boolean; labelClass?: string; disableLocalFiltering?: boolean; isLoading?: boolean; emptyStateMessage?: string; onAction?: (item: Item) => void; onSearch?: (query: string) => void; onEmpty?: () => void; thumbnail?: (item: Item) => ReactNode; icon?: (item: Item) => ReactNode; label?: (item: Item) => ReactNode; trailingIcon?: (item: Item) => ReactNode; footer?: ReactNode };
export default function DropdownMenu({ menuItems = [], menuSections = [], thumbnailSize = 20, roundedThumbnail = true, showSearch, searchPlaceholder = "Search...", isSearching, labelClass = "", disableLocalFiltering, isLoading, emptyStateMessage = "No results found.", onAction, onSearch, onEmpty, thumbnail, icon, label, trailingIcon, footer }: Props) {
  const [query, setQuery] = useState("");
  const filter = (items: Item[], value: string) => disableLocalFiltering || !value ? items : items.filter(item => item.label.toLowerCase().includes(value.toLowerCase()));
  const sections = menuSections.map(section => ({ ...section, items: filter(section.items || [], query) })).filter(section => disableLocalFiltering || !query || section.items.length);
  const items = filter(menuItems, query);
  const empty = menuSections.length ? !sections.length : !items.length;
  const renderItem = (item: Item, index: number) => <button key={index} type="button" className={`inline-flex items-center justify-start w-full h-8 min-w-0 gap-2 px-2 py-1.5 transition-all duration-200 ease-in-out border-0 rounded-lg z-60 hover:bg-n-alpha-1 dark:hover:bg-n-alpha-2 disabled:cursor-not-allowed disabled:pointer-events-none disabled:opacity-50 ${item.isSelected ? "bg-n-alpha-1 dark:bg-n-solid-active" : ""} ${item.action === "delete" ? "text-n-ruby-11" : "text-n-slate-12"}`} disabled={item.disabled} onClick={() => onAction?.(item)}>
    {thumbnail ? thumbnail(item) : item.thumbnail && <Avatar {...item.thumbnail} size={thumbnailSize} roundedFull={roundedThumbnail} />}
    {icon ? icon(item) : item.icon && <Icon icon={item.icon} className="flex-shrink-0 size-3.5" />}
    {item.emoji && <span className="flex-shrink-0 size-4" style={{ color: item.iconColor }}>{item.emoji}</span>}
    {label ? label(item) : item.label && <span className={`min-w-0 text-sm font-420 truncate ${labelClass}`}>{item.label}</span>}
    {trailingIcon?.(item)}
  </button>;
  return <div className="bg-n-alpha-3 backdrop-blur-[100px] border-0 outline outline-1 outline-n-container absolute rounded-xl z-50 flex flex-col min-w-[136px] shadow-lg pt-2 overflow-hidden">
    {showSearch && <div className="relative shrink-0 px-2 mb-2"><Icon icon="i-lucide-search" className="absolute size-3.5 top-2 ltr:left-5 rtl:right-5" /><input autoFocus type="search" aria-label={searchPlaceholder} placeholder={searchPlaceholder} value={query} className="reset-base w-full h-8 py-2 ltr:pl-10 ltr:pr-2 rtl:pl-2 rtl:pr-10 text-sm focus:outline-none border-none rounded-lg bg-n-alpha-black2 dark:bg-n-solid-1 text-n-slate-12" onChange={event => { const value = event.target.value; setQuery(value); onSearch?.(value); if (menuSections.length ? !menuSections.some(section => filter(section.items || [], value).length) : !filter(menuItems, value).length) onEmpty?.(); }} /></div>}
    <div className="flex flex-col gap-2 overflow-y-auto min-h-0 px-2 pb-2">{menuSections.length ? sections.map((section, index) => <div key={index} className="flex flex-col gap-1">{section.title && <p className="px-2 py-2 text-xs mb-0 font-medium text-n-slate-11 uppercase tracking-wide sticky top-0 z-10 bg-n-alpha-3 backdrop-blur-sm truncate min-w-0">{section.title}</p>}{section.isLoading ? <div className="flex items-center justify-center py-2"><Spinner size={24} /></div> : !section.items.length && section.emptyState && <div className="text-sm text-n-slate-11 px-2 py-1.5">{section.emptyState}</div>}{section.items.map(renderItem)}{index < sections.length - 1 && <div className="h-px bg-n-alpha-2 mx-2 my-1" />}</div>) : <>{isLoading && <div className="flex items-center justify-center py-2"><Spinner size={24} /></div>}{items.map(renderItem)}</>}{empty && <div className="text-sm text-n-slate-11 px-2 py-1.5">{isSearching ? "Searching..." : query ? "No results found." : emptyStateMessage}</div>}</div>
    {footer && <div className="shrink-0">{footer}</div>}
  </div>;
}
