<!DOCTYPE html>
<html lang="en">
    <head>
        <meta charset="utf-8">
        <meta name="viewport" content="width=device-width, initial-scale=1">
        <title>Deku Laravel Starter</title>
        <style>
            :root {
                color: #1b150f;
                background:
                    radial-gradient(circle at top right, rgba(244, 189, 124, 0.28), transparent 25rem),
                    linear-gradient(180deg, #fbf5ef 0%, #eddcc8 100%);
                font-family: "Instrument Serif", Georgia, serif;
            }

            * {
                box-sizing: border-box;
            }

            body {
                min-height: 100vh;
                margin: 0;
                display: grid;
                place-items: center;
                padding: 2rem;
            }

            main {
                max-width: 44rem;
                padding: 2.5rem;
                border-radius: 2rem;
                background: rgba(255, 250, 244, 0.92);
                box-shadow: 0 1rem 3rem rgba(45, 27, 9, 0.12);
            }

            p,
            li {
                font-family: "IBM Plex Sans", sans-serif;
                line-height: 1.65;
            }

            .eyebrow {
                text-transform: uppercase;
                letter-spacing: 0.12em;
                font-size: 0.8rem;
                color: #9a5d1c;
            }
        </style>
    </head>
    <body>
        <main>
            <p class="eyebrow">Deku Template</p>
            <h1>Laravel starter</h1>
            <p>
                This template keeps the framework current by generating a fresh Laravel skeleton
                during the Docker build, then applying a local view and route set for Deku.
            </p>
            <ul>
                <li>Builder: Dockerfile</li>
                <li>Health endpoint: /health</li>
                <li>Optional services: Deku Postgres, MySQL, Redis</li>
            </ul>
        </main>
    </body>
</html>
