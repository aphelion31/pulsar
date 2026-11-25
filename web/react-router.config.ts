import type { Config } from "@react-router/dev/config";

// SPA mode: static build in build/client, served by the Pulsar Rust binary.
export default {
  ssr: false,
  basename: "/app/",
} satisfies Config;
