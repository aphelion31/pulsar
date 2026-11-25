import { Fragment, useEffect, useLayoutEffect, useRef, useState } from "react";
import Icon from "./Icon";
type Tab = { label: string; count?: number; icon?: string; [key: string]: unknown };
export default function TabBar({ initialActiveTab = 0, tabs, onTabChanged }: { initialActiveTab?: number; tabs: Tab[]; onTabChanged?: (tab: Tab) => void }) {
  const refs = useRef<(HTMLButtonElement | null)[]>([]);
  const [indicator, setIndicator] = useState({ left: 0, width: 0 });
  const [transition, setTransition] = useState(false);
  useLayoutEffect(() => { const element = refs.current[initialActiveTab]; if (!element) return; const update = () => setIndicator({ left: element.offsetLeft, width: element.offsetWidth }); update(); const observer = new ResizeObserver(update); observer.observe(element); return () => observer.disconnect(); }, [initialActiveTab, tabs]);
  useEffect(() => { setTransition(true); }, []);
  return <div className="relative flex items-center h-8 rounded-lg bg-n-alpha-1 dark:bg-n-solid-1 w-fit transition-all duration-200 ease-out has-[button:active]:scale-[1.01]">
    <div className={`absolute rounded-lg bg-n-solid-active shadow-sm pointer-events-none h-8 outline-1 outline outline-n-container inset-y-0 ${transition ? "transition-all duration-300 ease-out" : ""}`} style={indicator} />
    {tabs.map((tab, index) => <Fragment key={index}><button ref={element => { refs.current[index] = element; }} type="button" className={`relative z-10 inline-flex items-center min-w-0 gap-1.5 px-4 py-1.5 text-sm border-0 outline-1 outline-transparent rounded-lg transition-all duration-200 ease-out hover:text-n-brand active:scale-[1.02] ${initialActiveTab === index ? "text-n-blue-11 scale-100" : "text-n-slate-10 scale-[0.98]"}`} onClick={() => onTabChanged?.(tab)}>{tab.icon && <Icon icon={tab.icon} className="size-4 shrink-0" />}<span className="truncate">{tab.label} {tab.count ? `(${tab.count})` : ""}</span></button>{index < tabs.length - 1 && <div className={`w-px h-3.5 rounded my-auto transition-colors duration-300 ease-in-out ${(index > initialActiveTab && index < tabs.length - 1) || index < initialActiveTab - 1 ? "bg-n-strong" : "bg-transparent dark:bg-transparent"}`} />}</Fragment>)}
  </div>;
}
