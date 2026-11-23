import { LoadingPage, SkeletonRows } from "@/components/ui";

/**
 * Shown the moment a page is opened, until its data arrives: a title, a
 * row of figures and a list, in grey. The real page fades in over it.
 */
export default function Loading() {
  return (
    <LoadingPage>
      <div>
        <span className="skel" style={{ height: 40, width: "min(320px, 60%)" }} />
        <span className="skel mt-3" style={{ height: 18, width: "min(460px, 85%)" }} />
      </div>
      <div className="grid grid-cols-2 gap-4 lg:grid-cols-4">
        {[0, 1, 2, 3].map((i) => (
          <span key={i} className="skel" style={{ height: 112, borderRadius: 22 }} />
        ))}
      </div>
      <div className="panel p-4 sm:p-6">
        <SkeletonRows />
      </div>
    </LoadingPage>
  );
}
