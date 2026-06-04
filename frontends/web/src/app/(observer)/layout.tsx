import { SiteChrome } from "@/components/SiteChrome";

/** Renders the observer layout component. */
export default async function ObserverLayout({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  return <SiteChrome>{children}</SiteChrome>;
}
