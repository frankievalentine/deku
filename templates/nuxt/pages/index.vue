<template>
  <main class="page">
    <section class="panel">
      <p class="eyebrow">Deku Template</p>
      <h1>Nuxt starter</h1>
      <p>
        A framework-default Nuxt app shaped for Deku source deploys with Nixpacks.
      </p>
      <ul>
        <li>Builder: Nixpacks</li>
        <li>Runtime: Nuxt SSR</li>
        <li>Health endpoint: /api/health</li>
      </ul>
    </section>
  </main>
</template>

<style scoped>
.page {
  min-height: 100vh;
  margin: 0;
  padding: 2rem;
  display: grid;
  place-items: center;
  background:
    linear-gradient(120deg, rgba(21, 113, 94, 0.18), rgba(10, 24, 33, 0) 50%),
    linear-gradient(180deg, #f0f7f4, #d9ece4);
  font-family: "Space Grotesk", sans-serif;
  color: #10221a;
}

.panel {
  max-width: 40rem;
  padding: 2rem;
  border-radius: 1.5rem;
  background: rgba(255, 255, 255, 0.9);
  box-shadow: 0 1rem 3rem rgba(16, 34, 26, 0.1);
}

.eyebrow {
  margin: 0 0 0.75rem;
  text-transform: uppercase;
  letter-spacing: 0.12em;
  font-size: 0.8rem;
  color: #15715e;
}

h1 {
  margin: 0 0 0.75rem;
  font-size: clamp(2rem, 4vw, 3.5rem);
}

p,
li {
  line-height: 1.6;
}
</style>
