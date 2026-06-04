import { SiteChrome } from "@/components/SiteChrome";

/** Renders the shared auth layout component. */
export default async function AuthLayout({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  return <SiteChrome>{children}</SiteChrome>;
}
