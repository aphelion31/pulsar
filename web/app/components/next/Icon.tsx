import * as Lucide from "lucide-react";
import { isValidElement } from "react";
import type { ComponentType, ReactElement, SVGProps } from "react";

type Props = SVGProps<SVGSVGElement> & { icon: string | ComponentType<SVGProps<SVGSVGElement>> | ReactElement };
export default function Icon({ icon, ...props }: Props) {
  if (!icon) return null;
  if (isValidElement(icon)) return icon;
  if (typeof icon !== "string") { const Component = icon as ComponentType<SVGProps<SVGSVGElement>>; return <Component {...props} />; }
  if (icon === "i-woot-logic-or") return <svg viewBox="0 0 24 24" fill="currentColor" {...props}><rect x="14" y="5" width="2" height="13" rx="1" /><rect x="8" y="5" width="2" height="13" rx="1" /></svg>;
  const name = icon.replace(/^i-lucide-/, "").replace(/(^|-)(\w)/g, (_, _prefix, char: string) => char.toUpperCase());
  const Component = (Lucide as unknown as Record<string, ComponentType<SVGProps<SVGSVGElement>>>)[name];
  return Component ? <Component {...props} /> : null;
}
