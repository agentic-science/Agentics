import type { NextConfig } from "next";
import createNextIntlPlugin from "next-intl/plugin";

const withNextIntl = createNextIntlPlugin("./src/i18n/request.ts");

const backendOrigin = (
  process.env.AGENTICS_API_BASE_URL ?? "http://127.0.0.1:3000"
).replace(/\/$/, "");

const nextConfig: NextConfig = {
  reactCompiler: true,
  allowedDevOrigins: ["127.0.0.1"],
  async rewrites() {
    return [
      {
        source: "/api/:path*",
        destination: `${backendOrigin}/api/:path*`,
      },
      {
        source: "/admin-api/:path*",
        destination: `${backendOrigin}/admin/:path*`,
      },
    ];
  },
};

export default withNextIntl(nextConfig);
