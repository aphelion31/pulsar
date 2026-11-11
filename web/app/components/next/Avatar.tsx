import { Upload, User, X } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import type { CSSProperties, ReactNode } from "react";

type Props = { src?: string; name: string; size?: number; allowUpload?: boolean; roundedFull?: boolean; status?: "online" | "busy" | "offline" | null; inbox?: Record<string, unknown> | null; iconName?: string | null; hideOfflineStatus?: boolean; badge?: ReactNode; overlay?: ReactNode; onUpload?: (value: { file: File; url: string }) => void; onDelete?: () => void };
const light = [["#FBDCEF", "#C2298A"], ["#FFE0BB", "#99543A"], ["#E8E8E8", "#60646C"], ["#CCF3EA", "#008573"], ["#EBEBFE", "#4747C2"], ["#E1E9FF", "#3A5BC7"]];
const dark = [["#4B143D", "#FF8DCC"], ["#3F220D", "#FFA366"], ["#2A2A2A", "#ADB1B8"], ["#023B37", "#0BD8B6"], ["#27264D", "#A19EFF"], ["#1D2E62", "#9EB1FF"]];
export default function Avatar({ src = "", name, size = 32, allowUpload, roundedFull, status, inbox, iconName, hideOfflineStatus, badge, overlay, onUpload, onDelete }: Props) {
  const [valid, setValid] = useState(true);
  const input = useRef<HTMLInputElement>(null);
  useEffect(() => setValid(true), [src]);
  const radius = roundedFull ? "rounded-full" : size <= 16 ? "rounded" : size <= 24 ? "rounded-md" : size <= 32 ? "rounded-lg" : size <= 48 ? "rounded-xl" : "rounded-2xl";
  const fallback = !src && !name;
  const index = name.length % light.length;
  const colors = { backgroundColor: light[index][0], color: light[index][1], "--dark-bg": dark[index][0], "--dark-text": dark[index][1] } as CSSProperties;
  const initials = name.replace(/\p{Extended_Pictographic}/gu, "").split(/\s+/).slice(0, 2).map(word => word.charAt(0)).join("").toUpperCase();
  const badgeSize = Math.max(size * 0.35, 8);
  const badgeStyle = { width: badgeSize, height: badgeSize, top: size - badgeSize / 1.1, left: size - badgeSize / 1.1 };
  const statusClass = status === "online" ? "bg-n-teal-10" : status === "busy" ? "bg-n-amber-10" : status === "offline" && !hideOfflineStatus ? "bg-n-slate-10" : "";
  return <span className="relative inline-flex group/avatar z-0 flex-shrink-0 align-middle" style={{ width: size, height: size }}>
    {badge ?? (statusClass ? <div className={`absolute z-20 border rounded-full border-n-slate-3 ${statusClass}`} style={badgeStyle} /> : inbox ? <div style={badgeStyle} className="absolute z-20 flex items-center justify-center rounded-full bg-n-solid-1 border border-transparent flex-shrink-0" /> : null)}
    {src && allowUpload && <button type="button" aria-label="Remove avatar" className="absolute z-20 flex items-center justify-center invisible w-6 h-6 transition-all duration-300 ease-in-out opacity-0 cursor-pointer outline outline-1 outline-n-container -top-2 ltr:-right-2 rtl:-left-2 rounded-xl bg-n-solid-3 group-hover/avatar:visible group-hover/avatar:opacity-100" onClick={event => { event.stopPropagation(); if (input.current) input.current.value = ""; onDelete?.(); }}><X className="text-n-slate-11 size-4" /></button>}
    <span role="img" aria-label={name || "Not available"} className={`relative inline-flex items-center justify-center object-cover overflow-hidden font-medium outline outline-1 -outline-offset-1 outline-[rgb(0_0_0_/_0.03)] dark:outline-[rgb(255_255_255_/_0.04)] ${radius} ${fallback ? "bg-n-slate-3 dark:bg-n-slate-4" : !src || !valid ? "dark:!bg-[var(--dark-bg)] dark:!text-[var(--dark-text)]" : ""}`} style={{ width: size, height: size, ...(!fallback && (!src || !valid) ? colors : {}) }}>
      {src && valid ? <img src={src} alt={name} onError={() => setValid(false)} /> : iconName ? <span className={iconName} style={{ fontSize: size / 1.6 }} /> : !fallback ? <span style={{ fontSize: Math.min(size / 2.5, 24) }} className="select-none">{initials}</span> : <User aria-label="Not available" style={{ width: size / 1.6, height: size / 1.6 }} />}
      {overlay ?? (allowUpload && <button type="button" aria-label="Upload avatar" className={`absolute inset-0 z-10 flex items-center justify-center invisible w-full h-full transition-all duration-300 ease-in-out opacity-0 bg-n-alpha-black1 group-hover/avatar:visible group-hover/avatar:opacity-100 ${radius}`} onClick={() => input.current?.click()}><Upload className="text-white" style={{ width: size / 2, height: size / 2 }} /></button>)}
      {allowUpload && <input ref={input} type="file" accept="image/png, image/jpeg, image/jpg, image/gif, image/webp" className="hidden" onChange={event => { const file = event.target.files?.[0]; if (file) onUpload?.({ file, url: URL.createObjectURL(file) }); }} />}
    </span>
  </span>;
}
