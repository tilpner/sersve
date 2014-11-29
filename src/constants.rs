pub const TEMPLATE: &'static str =
"<!DOCTYPE html>
<html>
    <title>{{title}}</title>
    <style type=\"text/css\">
* {
	padding:0;
	margin:0;
}

body {
	color: #333;
	font: 14px Sans-Serif;
	padding: 50px;
	background: #eee;
}

h1 {
	text-align: center;
	padding: 20px 0 12px 0;
	margin: 0;
}
h2 {
	font-size: 16px;
	text-align: center;
	padding: 0 0 12px 0;
}

#container {
	box-shadow: 0 5px 10px -5px rgba(0,0,0,0.5);
	position: relative;
	background: white;
}

table {
	background-color: #F3F3F3;
	border-collapse: collapse;
	width: 100%;
	margin: 15px 0;
}

th {
	background-color: #215fa4;
	color: #FFF;
	cursor: pointer;
	padding: 5px 10px;
}

th small {
	font-size: 9px;
}

td, th {
	text-align: left;
}

a {
	text-decoration: none;
}

td a {
	color: #001c3b;
	display: block;
	padding: 5px 10px;
}
th a {
	padding-left: 0
}

tr:nth-of-type(odd) {
	background-color: #E6E6E6;
}

tr:hover td {
	background-color:#CACACA;
}

tr:hover td a {
	color: #000;
}
    </style>
    <body>
        <div id=\"container\">
            <h1>{{title}}</h1>
            <table>
                <thead>
                    <tr>
                        <th>Name</th>
                        <th>Size</th>
                    </tr>
                </thead>
                <tbody>
                {{#content}}
                    <tr>
                        <td>
                            <a href=\"/{{url}}\">{{name}}</a>
                        </td>
                        <td>
                            {{size}}
                        </td>
                    </tr>
                {{/content}}
                </tbody>
            </table>
        </div>
    </body>
</html>";
